use std::{collections::HashMap, net::IpAddr};

use etherparse::{IpHeader, TcpHeader};
use pcap::{Capture, Device};

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
            IpHeader::Version4(_, _) => todo!(),
            IpHeader::Version6(ip_header, _) => ip_header.source.into(),
        };

        let dest_addr = match ip {
            IpHeader::Version4(_, _) => todo!(),
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

fn main() {
    let mut devices = Device::list().unwrap();
    let i = devices.iter().position(|d| d.name == "lo0").unwrap();
    let dev = devices.remove(i);
    let mut packet_count = 0u32;

    let mut cap = Capture::from_device(dev)
        .unwrap()
        .immediate_mode(true)
        .promisc(true)
        .open()
        .unwrap();

    let mut streams = Streams::new();

    while let Ok(packet) = cap.next_packet() {
        //println!("received {:?}", packet);

        let family = u32::from_le_bytes(packet.data[0..4].try_into().unwrap());

        //println!("Family: {}", family);

        if family == 30 {
            let (headers, _next_version, payload) =
                IpHeader::from_slice(&packet.data[4..]).unwrap();

            //println!("Headers {:?}", headers);

            let (tcp, payload2) = TcpHeader::from_slice(&payload).unwrap();

            //println!("Tcp {:?} payload {:?}", tcp, payload2);

            let stream = TcpStream::from_headers(&headers, &tcp);
            let (index, side) = streams.store(stream);

            println!("Stream index is {}", index);

            match side {
                EndpointSide::Source => streams.append_request_bytes(index, payload2),
                EndpointSide::Destination => streams.append_response_bytes(index, payload2),
            }

            if tcp.fin {
                let req = streams.take_request(index);

                println!("Request: {}", String::from_utf8(req).unwrap());

                let resp = streams.take_response(index);

                println!("Response: {}", String::from_utf8(resp).unwrap());
            }

            //if payload2.len() > 0 {
            //    println!("Payload: {}", String::from_utf8(payload2.into()).unwrap());
            //}

            packet_count += 1;
            if packet_count > 50 {
                break;
            }
        }
    }
}
