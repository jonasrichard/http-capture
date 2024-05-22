// Handling TCP streams (loopback and real interfaces)

use std::net::IpAddr;

use etherparse::{Ipv4Header, SlicedPacket, TcpHeader};

use crate::ui::stream::RawStream;

/// Identifies an endpoint.
#[derive(Debug, PartialEq)]
pub enum EndpointSide {
    Source,
    Destination,
}

#[derive(Debug, PartialEq)]
pub struct Endpoint {
    address: IpAddr,
    port: u16,
}

#[derive(Debug)]
pub struct Party {
    side: EndpointSide,
    endpoint: Endpoint,
}

pub struct TcpStream {
    id: usize,
    source: Party,
    destination: Party,
    request: Vec<u8>,
    response: Vec<u8>,
    fin: (bool, bool),
}

impl TcpStream {
    pub fn new(id: usize, source: Endpoint, destination: Endpoint) -> TcpStream {
        TcpStream {
            id,
            source: Party {
                side: EndpointSide::Source,
                endpoint: source,
            },
            destination: Party {
                side: EndpointSide::Destination,
                endpoint: destination,
            },
            request: vec![],
            response: vec![],
            fin: (false, false),
        }
    }

    pub fn append_request_bytes(&mut self, b: &[u8]) {
        self.request.extend_from_slice(b);
    }

    pub fn append_response_bytes(&mut self, b: &[u8]) {
        self.response.extend_from_slice(b);
    }

    pub fn set_source_fin(&mut self) {
        self.fin.0 = true;
    }

    pub fn set_destination_fin(&mut self) {
        self.fin.1 = true;
    }

    pub fn is_finished(&self) -> bool {
        self.fin == (true, true)
    }

    pub fn convert_to_raw_stream(self) -> RawStream {
        RawStream {
            id: self.id,
            request: self.request,
            response: self.response,
        }
    }

    pub fn from_sliced_packet(p: &SlicedPacket) -> Option<(Endpoint, Endpoint)> {
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

    pub fn extract_endpoint_pair(
        ip4_header: Ipv4Header,
        tcp_header: TcpHeader,
    ) -> Option<(Endpoint, Endpoint)> {
        Some((
            Endpoint {
                address: ip4_header.source.into(),
                port: tcp_header.source_port,
            },
            Endpoint {
                address: ip4_header.destination.into(),
                port: tcp_header.destination_port,
            },
        ))
    }

    /// Checks if the two endpoints as source and destination are in the tcp stream and gives back
    /// the `EndpointSide` as `Source` if the first endpoint is the source in the tcp stream and
    /// the second is the destination. If they are parties in the tcp stream but opposite order,
    /// the result will be `Destination`. Otherwise it gives back `None`.
    pub fn same_parties(&self, source: &Endpoint, destination: &Endpoint) -> Option<EndpointSide> {
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
