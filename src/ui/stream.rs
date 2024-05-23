use std::{collections::HashMap, net::IpAddr};

use ratatui::{
    style::{Color, Modifier, Style, Styled},
    text::{Line, Span, Text},
    widgets::Row,
};

/// RawStream is a set of packets collected by the capture module.
///
/// The packets here belong to the same conversation and the request
/// and the response are already normalized as byte arrays.
pub struct RawStream {
    pub id: usize,
    pub ts: i64,
    pub source_addr: IpAddr,
    pub source_port: u16,
    pub dest_addr: IpAddr,
    pub dest_port: u16,
    pub request: Vec<u8>,
    pub response: Vec<u8>,
}

#[derive(Clone)]
pub struct HttpStream {
    pub id: usize,
    pub timestamp: i64,
    pub source_addr: IpAddr,
    pub source_port: u16,
    pub dest_addr: IpAddr,
    pub dest_port: u16,
    pub parsed_request: Req,
    pub parsed_response: Resp,
}

#[derive(Clone)]
pub struct Req {
    pub method: String,
    pub path: String,
    pub version: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}

#[derive(Clone)]
pub struct Resp {
    pub version: String,
    pub code: u16,
    pub reason: Option<String>,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}

impl std::fmt::Debug for RawStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let to = std::cmp::min(self.response.len(), 64);
        let resp = String::from_utf8_lossy(&self.response[0..to]).to_string();

        f.debug_struct("RawStream")
            .field("id", &self.id)
            .field(
                "request",
                &String::from_utf8(self.request.clone()).unwrap_or("Non UTF-8 data".to_string()),
            )
            .field("response", &resp)
            .finish()
    }
}

impl TryFrom<RawStream> for HttpStream {
    type Error = &'static str;

    fn try_from(raw: RawStream) -> Result<Self, Self::Error> {
        // Parse request
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut parsed_req = httparse::Request::new(&mut headers);
        let res = parsed_req.parse(raw.request.as_slice()).unwrap();

        if res.is_partial() {
            return Err("Partial Request");
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

        if body_start < raw.request.len() {
            req.body = Some(
                String::from_utf8(raw.request[body_start..].to_vec())
                    .unwrap_or("Encoding error".to_string()),
            );
        }

        // Parse response
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut parsed_resp = httparse::Response::new(&mut headers);
        let res = parsed_resp.parse(raw.response.as_slice()).unwrap();

        if res.is_partial() {
            return Err("Partial Response");
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
                String::from_utf8(header.value.to_vec()).unwrap(),
            );
        }

        let body_start = res.unwrap();

        if body_start < raw.response.len() {
            resp.body = Some(
                String::from_utf8(raw.response[body_start..].to_vec())
                    .unwrap_or("Encoding error".to_string()),
            );
        }

        let req_len = match req.body {
            None => 0,
            Some(ref b) => b.len(),
        };

        let resp_len = match resp.body {
            None => 0,
            Some(ref b) => b.len(),
        };

        let stream = HttpStream {
            id: raw.id,
            parsed_request: req,
            parsed_response: resp,
            timestamp: raw.ts,
            source_addr: raw.source_addr,
            source_port: raw.source_port,
            dest_addr: raw.dest_addr,
            dest_port: raw.dest_port,
        };

        Ok(stream)
    }
}

impl From<HttpStream> for Row<'_> {
    fn from(value: HttpStream) -> Self {
        Row::new(vec![
            format!("{:10}", value.timestamp),
            format!("{}:{}", value.source_addr, value.source_port),
            format!("{}:{}", value.dest_addr, value.dest_port),
            format!(
                "{} {}",
                value.parsed_request.method, value.parsed_request.path
            ),
        ])
    }
}

impl HttpStream {
    pub fn write_to_text(&self, text: &mut Text) {
        let pr = &self.parsed_request;

        let green = Style::new().fg(Color::Green).add_modifier(Modifier::BOLD);

        text.push_line(Line::styled(format!("{} {}\n", pr.method, pr.path), green));

        let red = Style::new().fg(Color::LightRed);

        for header in &pr.headers {
            let mut line = Line::styled(format!("{}:", header.0), red);

            line.push_span(Span::styled(format!(" {}\n", header.1), Color::Gray));

            text.push_line(line);
        }

        text.extend(Text::raw("\n"));

        if let Some(ref body) = pr.body {
            text.extend(Text::raw(body.clone()));
        }

        text.extend(Text::raw("\n"));

        let resp = &self.parsed_response;

        text.push_line(Line::styled(
            format!("{} {}", resp.code, resp.version),
            green,
        ));

        for header in &resp.headers {
            let mut line = Line::styled(format!("{}:", header.0), red);

            line.push_span(Span::styled(format!(" {}\n", header.1), Color::Gray));

            text.push_line(line);
        }

        text.extend(Text::raw("\n"));

        if let Some(ref body) = resp.body {
            text.extend(Text::raw(body.clone()));
        }
    }
}
