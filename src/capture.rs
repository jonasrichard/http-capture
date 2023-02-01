use std::{
    net::IpAddr,
    thread::{self, JoinHandle},
};

use crossbeam::{
    channel::{self, Receiver, Sender},
    select,
};
use etherparse::{IpHeader, TcpHeader};
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
    fn from_headers(ip: &IpHeader, tcp: &TcpHeader) -> (Endpoint, Endpoint) {
        let source_addr = match ip {
            IpHeader::Version4(ip_header, _) => ip_header.source.into(),
            IpHeader::Version6(ip_header, _) => ip_header.source.into(),
        };

        let dest_addr = match ip {
            IpHeader::Version4(ip_header, _) => ip_header.destination.into(),
            IpHeader::Version6(ip_header, _) => ip_header.destination.into(),
        };

        (
            Endpoint {
                address: source_addr,
                port: tcp.source_port,
            },
            Endpoint {
                address: dest_addr,
                port: tcp.destination_port,
            },
        )
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
        let mut i = 0;

        for stream in &self.streams {
            match stream.same_parties(source, dest) {
                None => {}
                Some(EndpointSide::Source) => {
                    return Some((i, EndpointSide::Source));
                }
                Some(EndpointSide::Destination) => {
                    return Some((i, EndpointSide::Destination));
                }
            }

            i += 1;
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
        capture_loop(device, output, commands);
    })
}

fn packet_stream(mut cap: Capture<Active>) -> Receiver<Vec<u8>> {
    let (tx, rx) = channel::bounded(10);

    thread::spawn(move || {
        while let Ok(packet) = cap.next_packet() {
            let family = u32::from_le_bytes(packet.data[0..4].try_into().unwrap());

            if family == 30 || family == 2 {
                match tx.send(packet.data.to_vec()) {
                    Err(e) => {
                        //eprintln!("Error during sending {:?}", e);
                        break;
                    }
                    Ok(_) => (),
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

                let (headers, _next_version, payload) =
                    IpHeader::from_slice(&packet.as_slice()[4..]).unwrap();

                if let Ok((tcp, payload2)) = TcpHeader::from_slice(&payload) {

                    //hexdump(&packet.as_slice());

                    let (src, dest) = TcpStream::from_headers(&headers, &tcp);
                    let (index, side) = streams.store(src, dest);

                    match side {
                        EndpointSide::Source => streams.append_request_bytes(index, payload2),
                        EndpointSide::Destination => streams.append_response_bytes(index, payload2),
                    }

                    // TODO Store if tcp fin came from source or dest side and mark that stream only, not take
                    // that. And also send to the stream.
                    // Rename struct, a lot of has name stream.
                    if tcp.fin {
                        if streams.register_fin(index, side) {
                            let stream = streams.send_stream(index);

                            //println!("{:?}", streams);

                            output
                                .send(stream)
                                .unwrap();
                        }
                    }
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
