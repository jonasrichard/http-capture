use std::{
    net::IpAddr,
    thread::{self, JoinHandle},
};

use crossbeam::{
    channel::{self, Receiver, Sender},
    select,
};
use etherparse::SlicedPacket;
use log::info;
use pcap::{Active, Capture, Device};

use crate::ui::RawStream;

#[derive(Debug, PartialEq)]
enum EndpointSide {
    Source,
    Destination,
}

#[derive(Debug, PartialEq)]
struct Endpoint {
    address: IpAddr,
    port: u16,
}

#[derive(Debug)]
struct Party {
    side: EndpointSide,
    endpoint: Endpoint,
}

struct TcpStream {
    id: usize,
    source: Party,
    destination: Party,
    request: Vec<u8>,
    response: Vec<u8>,
    fin: (bool, bool),
}

impl TcpStream {
    fn from_sliced_packet(p: &SlicedPacket) -> Option<(Endpoint, Endpoint)> {
        use etherparse::{InternetSlice, TransportSlice};

        let (src_port, dst_port) = match &p.transport {
            Some(TransportSlice::Tcp(tcp)) => (tcp.source_port(), tcp.destination_port()),
            _ => return None,
        };

        let (src_addr, dst_addr) = match &p.net {
            Some(InternetSlice::Ipv4(ip4)) => (
                IpAddr::V4(ip4.header().source_addr()),
                IpAddr::V4(ip4.header().destination_addr()),
            ),
            Some(InternetSlice::Ipv6(ip6)) => (
                IpAddr::V6(ip6.header().source_addr()),
                IpAddr::V6(ip6.header().destination_addr()),
            ),
            _ => return None,
        };

        Some((
            Endpoint {
                address: src_addr,
                port: src_port,
            },
            Endpoint {
                address: dst_addr,
                port: dst_port,
            },
        ))
    }

    /// Checks if the two endpoints as source and destination are in the tcp stream and gives back
    /// the `EndpointSide` as `Source` if the first endpoint is the source in the tcp stream and
    /// the second is the destination. If they are parties in the tcp stream but opposite order,
    /// the result will be `Destination`. Otherwise it gives back `None`.
    fn same_parties(&self, source: &Endpoint, destination: &Endpoint) -> Option<EndpointSide> {
        if &self.source.endpoint == source && &self.destination.endpoint == destination {
            Some(EndpointSide::Source)
        } else if &self.source.endpoint == destination && &self.destination.endpoint == source {
            Some(EndpointSide::Destination)
        } else {
            None
        }
    }
}

impl std::fmt::Debug for TcpStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TcpStream")
            .field("id", &self.id)
            .field("source", &self.source)
            .field("destination", &self.destination)
            .finish()
    }
}

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
                self.streams.push(TcpStream {
                    id: self.next_id,
                    source: Party {
                        side: EndpointSide::Source,
                        endpoint: source,
                    },
                    destination: Party {
                        side: EndpointSide::Destination,
                        endpoint: dest,
                    },
                    request: vec![],
                    response: vec![],
                    fin: (false, false),
                });

                self.next_id += 1;

                (self.streams.len() - 1, EndpointSide::Source)
            }
            Some(ep) => ep,
        }
    }

    fn append_request_bytes(&mut self, index: usize, bytes: &[u8]) {
        match self.streams.get_mut(index) {
            Some(stream) => stream.request.extend_from_slice(bytes),
            None => {
                panic!("Index {} cannot be found", index);
            }
        }
    }

    fn append_response_bytes(&mut self, index: usize, bytes: &[u8]) {
        match self.streams.get_mut(index) {
            Some(stream) => stream.response.extend_from_slice(bytes),
            None => {
                panic!("Index {} cannot be found", index);
            }
        }
    }

    fn register_fin(&mut self, index: usize, side: EndpointSide) -> bool {
        if let Some(stream) = self.streams.get_mut(index) {
            match side {
                EndpointSide::Source => stream.fin.0 = true,
                EndpointSide::Destination => stream.fin.1 = true,
            }

            stream.fin == (true, true)
        } else {
            false
        }
    }

    fn send_stream(&mut self, index: usize) -> RawStream {
        if let Some(stream) = self.streams.get_mut(index) {
            let mut raw = RawStream {
                id: stream.id,
                request: vec![],
                response: vec![],
            };

            std::mem::swap(&mut raw.request, &mut stream.request);
            std::mem::swap(&mut raw.response, &mut stream.response);

            raw
        } else {
            panic!("Invalid index {}", index);
        }
    }
}

pub enum Command {
    StopCapture,
}

pub fn start_capture(
    interface: String,
    output: Sender<RawStream>,
    commands: Receiver<Command>,
) -> JoinHandle<()> {
    let mut devices = Device::list().unwrap();
    let i = devices.iter().position(|d| d.name == interface).unwrap();
    let device = devices.remove(i);

    thread::spawn(move || {
        info!("Start capturing on {device:?}");

        capture_loop(device, output, commands);
    })
}

struct FilteredStream {
    src: Endpoint,
    dest: Endpoint,
    payload: Vec<u8>,
    fin: bool,
}

fn packet_stream(mut cap: Capture<Active>) -> Receiver<FilteredStream> {
    let (tx, rx) = channel::bounded(5);

    thread::spawn(move || {
        while let Ok(packet) = cap.next_packet() {
            //hexdump(&packet.data);

            let packet = SlicedPacket::from_ethernet(packet.data).unwrap();

            if let Some((source, dest)) = TcpStream::from_sliced_packet(&packet) {
                // TODO implement filtering here

                if source.port != 9000 && dest.port != 9000 {
                    continue;
                }

                if let Some(etherparse::TransportSlice::Tcp(tcp)) = packet.transport {
                    let filtered_stream = FilteredStream {
                        src: source,
                        dest,
                        payload: tcp.payload().to_vec(),
                        fin: tcp.fin(),
                    };

                    if let Err(e) = tx.send(filtered_stream) {
                        eprintln!("Error during sending {e:?}");

                        break;
                    }
                }
            }
        }
    });

    rx
}

fn capture_loop(device: Device, output: Sender<RawStream>, commands: Receiver<Command>) {
    let cap = Capture::from_device(device)
        .unwrap()
        .immediate_mode(true)
        .promisc(true)
        .open()
        .unwrap();

    let mut streams = Streams::new();

    let packets = packet_stream(cap);

    loop {
        select! {
            recv(packets) -> packet => {
                let packet = packet.unwrap();

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

                    output
                        .send(stream)
                        .unwrap();
                }
            }
            recv(commands) -> cmd => {
                match cmd {
                    Ok(Command::StopCapture) => {
                        return;
                    }
                    _ => {}
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

    println!("");
}
