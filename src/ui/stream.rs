use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
    net::IpAddr,
};

use flate2::read::MultiGzDecoder;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::Row,
};

pub struct HttpStream {
    pub id: usize,
    pub timestamp: i64,
    pub source_addr: IpAddr,
    pub source_port: u16,
    pub dest_addr: IpAddr,
    pub dest_port: u16,
    pub request: Vec<u8>,
    pub response: Vec<u8>,
    pub parsed_request: Option<Req>,
    pub parsed_response: Option<Resp>,
}

pub struct Req {
    pub method: String,
    pub path: String,
    pub version: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}

pub struct Resp {
    pub version: String,
    pub code: u16,
    pub reason: Option<String>,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}

impl std::fmt::Debug for HttpStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let to = std::cmp::min(self.response.len(), 64);
        let resp = String::from_utf8_lossy(&self.response[0..to]).to_string();

        f.debug_struct("HttpStream")
            .field("id", &self.id)
            .field(
                "request",
                &String::from_utf8(self.request.clone()).unwrap_or("Non UTF-8 data".to_string()),
            )
            .field("response", &resp)
            .finish()
    }
}

impl HttpStream {
    pub fn parse_request(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut parsed_req = httparse::Request::new(&mut headers);
        let res = parsed_req.parse(self.request.as_slice())?;

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

        if body_start < self.request.len() {
            req.body = Some(
                String::from_utf8(self.request[body_start..].to_vec())
                    .unwrap_or("Encoding error".to_string()),
            );
        }

        self.parsed_request = Some(req);

        Ok(())
    }

    pub fn parse_response(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut parsed_resp = httparse::Response::new(&mut headers);
        let res = parsed_resp.parse(self.response.as_slice()).unwrap();

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

        if body_start < self.response.len() {
            if let Some(enc) = resp.headers.get("Content-Encoding") {
                if enc == "gzip" {
                    resp.body = Some(unzip_content(&self.response[body_start..])?);
                } else {
                    return Err(format!("Unknown encoding {enc}").into());
                }
            } else {
                resp.body = Some(
                    String::from_utf8(self.response[body_start..].to_vec())
                        .unwrap_or("Encoding error".to_string()),
                );
            }
        }

        self.parsed_response = Some(resp);

        Ok(())
    }
}

impl From<&HttpStream> for Row<'_> {
    fn from(value: &HttpStream) -> Self {
        Row::new(vec![
            format!("{:10}", value.timestamp),
            format!("{}:{}", value.source_addr, value.source_port),
            format!("{}:{}", value.dest_addr, value.dest_port),
            match value.parsed_request {
                None => "Cannot parse request".to_string(),
                Some(ref pr) => format!("{} {}", pr.method, pr.path),
            },
        ])
    }
}

impl HttpStream {
    pub fn write_to_text(&self, text: &mut Text) {
        let green = Style::new().fg(Color::Green).add_modifier(Modifier::BOLD);
        let red = Style::new().fg(Color::LightRed);

        if let Some(ref pr) = self.parsed_request {
            text.push_line(Line::styled(format!("{} {}\n", pr.method, pr.path), green));

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
        }

        if let Some(ref resp) = self.parsed_response {
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

    pub fn write_to_file(
        &self,
        mut writer: std::io::BufWriter<File>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref req) = self.parsed_request {
            writer.write_fmt(format_args!(
                "HTTP 1.{} {} {}\n",
                req.version, req.method, req.path
            ))?;

            for header in &req.headers {
                writer.write_fmt(format_args!("{}: {}\n", header.0, header.1))?;
            }

            writer.write_all("\n".as_bytes())?;

            if let Some(ref body) = req.body {
                writer.write_all(body.as_bytes())?;
            }
        }

        if let Some(ref resp) = self.parsed_response {
            writer.write_fmt(format_args!("{}\n", resp.code))?;

            for header in &resp.headers {
                writer.write_fmt(format_args!("{}: {}\n", header.0, header.1))?;
            }

            writer.write_all("\n".as_bytes())?;

            if let Some(ref body) = resp.body {
                writer.write_all(body.as_bytes())?;
            }
        }

        Ok(())
    }
}

fn unzip_content(buf: &[u8]) -> Result<String, std::io::Error> {
    let mut gz = MultiGzDecoder::new(buf);
    let mut s = String::new();

    gz.read_to_string(&mut s)?;

    Ok(s)
}
