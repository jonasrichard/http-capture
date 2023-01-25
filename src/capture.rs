use std::{
    collections::HashMap,
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
struct Endpoint {
    address: IpAddr,
    port: u16,
}

enum EndpointSide {
    Source,
    Destination,
}

#[derive(Debug)]
struct TcpStream {
    source: Endpoint,
    destination: Endpoint,
}

impl TcpStream {
    fn from_headers(ip: &IpHeader, tcp: &TcpHeader) -> Self {
        let source_addr = match ip {
            IpHeader::Version4(ip_header, _) => ip_header.source.into(),
            IpHeader::Version6(ip_header, _) => ip_header.source.into(),
        };

        let dest_addr = match ip {
            IpHeader::Version4(ip_header, _) => ip_header.destination.into(),
            IpHeader::Version6(ip_header, _) => ip_header.destination.into(),
        };

        Self {
            source: Endpoint {
                address: source_addr,
                port: tcp.source_port,
            },
            destination: Endpoint {
                address: dest_addr,
                port: tcp.destination_port,
            },
        }
    }

    fn same_parties(&self, other: &TcpStream) -> Option<EndpointSide> {
        if self.source == other.source && self.destination == other.destination {
            Some(EndpointSide::Source)
        } else if self.source == other.destination && self.destination == other.source {
            Some(EndpointSide::Destination)
        } else {
            None
        }
    }
}

struct Streams {
    count: u32,
    streams: Vec<(u32, TcpStream)>,
    requests: HashMap<u32, Vec<u8>>,
    responses: HashMap<u32, Vec<u8>>,
}

impl Streams {
    fn new() -> Self {
        Self {
            count: 0u32,
            streams: vec![],
            requests: HashMap::new(),
            responses: HashMap::new(),
        }
    }

    fn lookup_stream(&self, ep: &TcpStream) -> Option<(u32, EndpointSide)> {
        for (index, stream) in &self.streams {
            match stream.same_parties(ep) {
                None => {}
                Some(EndpointSide::Source) => {
                    return Some((*index, EndpointSide::Source));
                }
                Some(EndpointSide::Destination) => {
                    return Some((*index, EndpointSide::Destination));
                }
            }
        }

        None
    }

    fn store(&mut self, stream: TcpStream) -> (u32, EndpointSide) {
        match self.lookup_stream(&stream) {
            None => {
                let num = self.count;

                self.streams.push((num, stream));

                self.count += 1;

                (num, EndpointSide::Source)
            }
            Some(ep) => ep,
        }
    }

    fn append_request_bytes(&mut self, index: u32, bytes: &[u8]) {
        match self.requests.get_mut(&index) {
            Some(request) => request.extend_from_slice(bytes),
            None => {
                self.requests.insert(index, bytes.to_vec());
            }
        }
    }

    fn append_response_bytes(&mut self, index: u32, bytes: &[u8]) {
        match self.responses.get_mut(&index) {
            Some(response) => response.extend_from_slice(bytes),
            None => {
                self.responses.insert(index, bytes.to_vec());
            }
        }
    }

    fn take_request(&mut self, index: u32) -> Vec<u8> {
        match self.requests.remove(&index) {
            Some(req) => req,
            None => vec![],
        }
    }

    fn take_response(&mut self, index: u32) -> Vec<u8> {
        match self.responses.remove(&index) {
            Some(resp) => resp,
            None => vec![],
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
                tx.send(packet.data.to_vec()).unwrap();
            }
        }
    });

    rx
}

fn capture_loop(device: Device, output: Sender<RawStream>, commands: Receiver<Command>) {
    let mut packet_count = 0u32;

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

                    let stream = TcpStream::from_headers(&headers, &tcp);
                    let (index, side) = streams.store(stream);

                    match side {
                        EndpointSide::Source => streams.append_request_bytes(index, payload2),
                        EndpointSide::Destination => streams.append_response_bytes(index, payload2),
                    }

                    // TODO Store if tcp fin came from source or dest side and mark that stream only, not take
                    // that. And also send to the stream.
                    // Rename struct, a lot of has name stream.
                    if tcp.fin {
                        let req = streams.take_request(index);
                        let resp = streams.take_response(index);

                        output
                            .send(RawStream {
                                id: 0,
                                request: req,
                                response: resp,
                            })
                            .unwrap();
                    }

                    packet_count += 1;
                    if packet_count > 50 {
                        return;
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
