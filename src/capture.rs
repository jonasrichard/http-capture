use crossbeam::{
    channel::{self, Receiver, Sender},
    select,
};
use etherparse::{Ipv4Header, SlicedPacket, TcpHeader};
use log::{error, info};
use pcap::{Active, Capture, Device};
use std::thread::{self, JoinHandle};

use crate::stream::{Endpoint, EndpointSide, TcpStream};
use crate::ui::stream::RawStream;

/// The stream storage where streams stored as a single vector and when
/// a new endpoint pair comes we can say if there is already a living
/// conversation between the two endpoints or it is something new.
#[derive(Debug)]
struct Streams {
    next_id: usize,
    streams: Vec<TcpStream>,
}

impl Streams {
    fn new() -> Self {
        Self {
            next_id: 0,
            streams: vec![],
        }
    }

    // TODO pass and endpoint pair rather?
    fn lookup_stream(&self, source: &Endpoint, dest: &Endpoint) -> Option<(usize, EndpointSide)> {
        for (i, stream) in self.streams.iter().enumerate() {
            match stream.same_parties(source, dest) {
                None => {}
                Some(EndpointSide::Source) => {
                    return Some((i, EndpointSide::Source));
                }
                Some(EndpointSide::Destination) => {
                    return Some((i, EndpointSide::Destination));
                }
            }
        }

        None
    }

    fn store(&mut self, source: Endpoint, dest: Endpoint) -> (usize, EndpointSide) {
        match self.lookup_stream(&source, &dest) {
            None => {
                let stream = TcpStream::new(self.next_id, source, dest);

                self.streams.push(stream);
                self.next_id += 1;

                (self.streams.len() - 1, EndpointSide::Source)
            }
            Some(ep) => ep,
        }
    }

    fn append_request_bytes(&mut self, index: usize, bytes: &[u8]) {
        match self.streams.get_mut(index) {
            Some(stream) => stream.append_request_bytes(bytes),
            None => {
                panic!("Index {} cannot be found", index);
            }
        }
    }

    fn append_response_bytes(&mut self, index: usize, bytes: &[u8]) {
        match self.streams.get_mut(index) {
            Some(stream) => stream.append_response_bytes(bytes),
            None => {
                panic!("Index {} cannot be found", index);
            }
        }
    }

    fn register_fin(&mut self, index: usize, side: EndpointSide) -> bool {
        if let Some(stream) = self.streams.get_mut(index) {
            match side {
                EndpointSide::Source => stream.set_source_fin(),
                EndpointSide::Destination => stream.set_destination_fin(),
            }

            stream.is_finished()
        } else {
            false
        }
    }

    fn send_stream(&mut self, index: usize) -> RawStream {
        let stream = self.streams.remove(index);

        stream.convert_to_raw_stream()
    }
}

pub enum Command {
    StopCapture,
}

pub fn start_capture(
    interface: String,
    port: u16,
    output: Sender<RawStream>,
    commands: Receiver<Command>,
) -> JoinHandle<()> {
    let mut devices = Device::list().unwrap();
    let i = devices.iter().position(|d| d.name == interface).unwrap();
    let device = devices.remove(i);

    thread::spawn(move || {
        let device_name = device.name.to_string();

        info!("Start capturing on {device:?}");

        capture_loop(device, port, output, commands);

        info!("Stop capturing on {device_name}");
    })
}

struct FilteredStream {
    src: Endpoint,
    dest: Endpoint,
    payload: Vec<u8>,
    fin: bool,
}

impl std::fmt::Debug for FilteredStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilteredStream")
            .field("src", &self.src)
            .field("dest", &self.dest)
            .field("payload", &self.payload.len())
            .field("fin", &self.fin)
            .finish()
    }
}

fn from_loopback_packet(data: &[u8]) -> (Ipv4Header, TcpHeader, &[u8]) {
    if data.starts_with(&[2, 0, 0, 0]) {
        // The next 20 bytes is the IP header
        let (ip4_header, _rest) = Ipv4Header::from_slice(&data[4..24]).unwrap();
        let (transport_header, rest) = TcpHeader::from_slice(&data[24..]).unwrap();

        (ip4_header, transport_header, rest)
    } else {
        panic!("Unknown sequence {}", data[0]);
    }
}

fn packet_stream(mut cap: Capture<Active>, loopback: bool) -> Receiver<FilteredStream> {
    let (tx, rx) = channel::bounded(5);

    info!("Starting packet stream");

    thread::spawn(move || {
        while let Ok(packet) = cap.next_packet() {
            //hexdump(&packet.data);

            if loopback {
                let (ip4, tcp, payload) = from_loopback_packet(packet.data);
                let tcp_fin = tcp.fin;

                if let Some((source, dest)) = TcpStream::extract_endpoint_pair(ip4, tcp) {
                    let filtered_stream = FilteredStream {
                        src: source,
                        dest,
                        payload: payload.to_vec(),
                        fin: tcp_fin,
                    };

                    if let Err(e) = tx.send(filtered_stream) {
                        error!("Error during sending {e:?}");

                        break;
                    }
                }
            } else {
                let packet = SlicedPacket::from_ethernet(packet.data).unwrap();

                info!("Stream {packet:?}");

                if let Some((source, dest)) = TcpStream::from_sliced_packet(&packet) {
                    // TODO implement filtering here

                    if let Some(etherparse::TransportSlice::Tcp(tcp)) = packet.transport {
                        let filtered_stream = FilteredStream {
                            src: source,
                            dest,
                            payload: tcp.payload().to_vec(),
                            fin: tcp.fin(),
                        };

                        if let Err(e) = tx.send(filtered_stream) {
                            error!("Error during sending {e:?}");

                            break;
                        }
                    }
                }
            }
        }
    });

    rx
}

fn capture_loop(device: Device, port: u16, output: Sender<RawStream>, commands: Receiver<Command>) {
    // Loopback packets are parsed somehow else
    let loopback = device.flags.is_loopback();

    let mut cap = match Capture::from_device(device)
        .unwrap()
        .immediate_mode(true)
        .open()
    {
        Ok(c) => c,
        Err(e) => {
            error!("Error {e:?}");

            return;
        }
    };

    if let Err(e) = cap.filter(format!("tcp port {port}").as_str(), true) {
        error!("Error {e:?}");

        return;
    }

    let mut streams = Streams::new();

    let packets = packet_stream(cap, loopback);

    loop {
        select! {
            recv(packets) -> packet => {
                let packet = packet.unwrap();

                info!("Packet {packet:?}");

                let (index, side) = streams.store(packet.src, packet.dest);

                match side {
                    EndpointSide::Source => streams.append_request_bytes(index, packet.payload.as_slice()),
                    EndpointSide::Destination => streams.append_response_bytes(index, packet.payload.as_slice()),
                }

                // TODO Store if tcp fin came from source or dest side and mark that stream only, not take
                // that. And also send to the stream.
                // Rename struct, a lot of has name stream.
                if packet.fin && streams.register_fin(index, side) {
                    let stream = streams.send_stream(index);

                    //println!("{:?}", streams);

                    if let Err(e) = output.send(stream) {
                        error!("Error {e:?}");
                    }
                }
            }
            recv(commands) -> cmd => {
                match cmd {
                    Ok(Command::StopCapture) => {
                        return;
                    }
                    Err(e) => {
                        error!("Error {e:?}");

                        return;
                    }
                }
            }
        }
    }
}

#[allow(dead_code)]
fn hexdump(data: &[u8]) {
    for ch in data.chunks(16) {
        let mut line = String::from("");

        for c in ch {
            print!("{:02X} ", c);

            if *c > 31 && *c < 128 {
                line.push(char::from_u32(*c as u32).unwrap_or('.'));
            } else {
                line.push('.');
            }
        }

        println!("|{}|", line);
    }

    println!();
}
