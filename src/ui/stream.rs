use std::{collections::HashMap, fs::File, io::Write, net::IpAddr};

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
        f.debug_struct("HttpStream")
            .field("id", &self.id)
            .field("source_addr", &self.source_addr)
            .field("source_port", &self.source_port)
            .field("dest_addr", &self.dest_addr)
            .field("dest_port", &self.dest_port)
            .finish()
    }
}

impl From<&HttpStream> for Row<'_> {
    fn from(value: &HttpStream) -> Self {
        Row::new(vec![
            format!("{:5}", value.id),
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
