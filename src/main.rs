use std::net::IpAddr;

use etherparse::{IpHeader, Ipv6HeaderSlice, TcpHeader};
use pcap::{Capture, Device};

#[derive(Debug, PartialEq)]
struct Endpoint {
    address: IpAddr,
    port: u16,
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

    fn same_parties(&self, other: &TcpStream) -> bool {
        (self.source == other.source && self.destination == other.destination)
            || (self.source == other.destination && self.destination == other.source)
    }
}

struct Streams {
    count: u32,
    streams: Vec<(u32, TcpStream)>,
}

impl Streams {
    fn new() -> Self {
        Self {
            count: 0u32,
            streams: vec![],
        }
    }

    fn lookup_stream(&self, ep: &TcpStream) -> Option<u32> {
        self.streams
            .iter()
            .find(|(_, st)| st.same_parties(ep))
            .map(|s| s.0)
    }

    fn store(&mut self, stream: TcpStream) -> u32 {
        match self.lookup_stream(&stream) {
            None => {
                let num = self.count;

                self.streams.push((num, stream));

                self.count += 1;

                num
            }
            Some(num) => num,
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

            println!("Headers {:?}", headers);

            let (tcp, payload2) = TcpHeader::from_slice(&payload).unwrap();

            println!("Tcp {:?} payload {:?}", tcp, payload2);

            let stream = TcpStream::from_headers(&headers, &tcp);
            let stream_index = streams.store(stream);

            println!("Stream index is {}", stream_index);

            if payload2.len() > 0 {
                println!("Payload: {}", String::from_utf8(payload2.into()).unwrap());
            }

            packet_count += 1;
            if packet_count > 50 {
                break;
            }
        }
    }
}
