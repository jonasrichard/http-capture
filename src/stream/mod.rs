// Handling TCP streams (loopback and real interfaces)

use std::{collections::HashMap, io::Read, net::IpAddr, str::FromStr};

use bytes::BytesMut;
use etherparse::{Ipv4Header, SlicedPacket, TcpHeader};
use flate2::read::MultiGzDecoder;

use crate::ui::stream::{HttpStream, Req, Resp};

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
    timestamp: i64,
    source: Party,
    destination: Party,
    request: BytesMut,
    response: BytesMut,
    fin: (bool, bool),
}

impl TcpStream {
    pub fn new(id: usize, timestamp: i64, source: Endpoint, destination: Endpoint) -> TcpStream {
        TcpStream {
            id,
            timestamp,
            source: Party {
                side: EndpointSide::Source,
                endpoint: source,
            },
            destination: Party {
                side: EndpointSide::Destination,
                endpoint: destination,
            },
            request: BytesMut::new(),
            response: BytesMut::new(),
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

    pub fn convert_to_http_stream(mut self) -> Vec<HttpStream> {
        let mut streams = vec![];

        loop {
            if let Ok(parsed_request) = parse_request(&mut self.request) {
                if let Ok(parsed_response) = parse_response(&mut self.response) {
                    let stream = HttpStream {
                        id: self.id,
                        timestamp: self.timestamp,
                        source_addr: self.source.endpoint.address,
                        source_port: self.source.endpoint.port,
                        dest_addr: self.destination.endpoint.address,
                        dest_port: self.destination.endpoint.port,
                        parsed_request: Some(parsed_request),
                        parsed_response: Some(parsed_response),
                    };

                    streams.push(stream);
                } else {
                    return streams;
                }
            } else {
                return streams;
            }
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

/// Parse the request headers and request body as well. Advance the `BytesMut` buffer
/// according to Content-Length header.
fn parse_request(req_bytes: &mut BytesMut) -> Result<Req, Box<dyn std::error::Error>> {
    let mut headers = [httparse::EMPTY_HEADER; 16];
    let mut parsed_req = httparse::Request::new(&mut headers);
    let res = parsed_req.parse(req_bytes)?;

    if res.is_partial() {
        return Err("Partial request".into());
    }

    let mut req = Req {
        method: parsed_req.method.unwrap().to_string(),
        path: parsed_req.path.unwrap().to_string(),
        version: parsed_req.version.unwrap().to_string(),
        headers: HashMap::new(),
        body: None,
    };

    for header in parsed_req.headers {
        req.headers.insert(
            header.name.to_string(),
            String::from_utf8(header.value.to_vec()).unwrap(),
        );
    }

    let body_start = res.unwrap();

    let _ = req_bytes.split_to(body_start);

    if let Some(content_length) = get_content_length(&req.headers) {
        if content_length > 0 {
            let body_buf = req_bytes.split_to(content_length);

            let body =
                String::from_utf8(body_buf.to_vec()).unwrap_or("Body encoding error".to_string());

            req.body = Some(body);
        }
    }

    Ok(req)
}

fn parse_response(resp_bytes: &mut BytesMut) -> Result<Resp, Box<dyn std::error::Error>> {
    let mut headers = [httparse::EMPTY_HEADER; 16];
    let mut parsed_resp = httparse::Response::new(&mut headers);
    let res = parsed_resp.parse(resp_bytes).unwrap();

    if res.is_partial() {
        return Err("Partial response".into());
    }

    let mut resp = Resp {
        version: parsed_resp.version.unwrap().to_string(),
        code: parsed_resp.code.unwrap(),
        reason: parsed_resp.reason.map(|r| r.to_string()),
        headers: HashMap::new(),
        body: None,
    };

    for header in parsed_resp.headers {
        resp.headers.insert(
            header.name.to_string(),
            String::from_utf8(header.value.to_vec())?,
        );
    }

    let body_start = res.unwrap();

    let _ = resp_bytes.split_to(body_start);

    if let Some(content_length) = get_content_length(&resp.headers) {
        let body_buf = resp_bytes.split_to(content_length);

        if let Some(enc) = resp.headers.get("Content-Encoding") {
            if enc == "gzip" {
                resp.body = Some(unzip_content(&body_buf)?);
            } else {
                return Err(format!("Unknown encoding {enc}").into());
            }
        } else {
            resp.body =
                Some(String::from_utf8(body_buf.to_vec()).unwrap_or("Encoding error".to_string()));
        }
    }

    Ok(resp)
}

fn get_content_length(headers: &HashMap<String, String>) -> Option<usize> {
    for (k, v) in headers {
        if "content-length" == k.to_lowercase() {
            return usize::from_str(v).ok();
        }
    }

    None
}

fn unzip_content(buf: &[u8]) -> Result<String, std::io::Error> {
    let mut gz = MultiGzDecoder::new(buf);
    let mut s = String::new();

    gz.read_to_string(&mut s)?;

    Ok(s)
}
