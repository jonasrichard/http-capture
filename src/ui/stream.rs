use std::collections::HashMap;

use ratatui::{text::Text, widgets::ListItem};

/// RawStream is a set of packets collected by the capture module.
///
/// The packets here belong to the same conversation and the request
/// and the response are already normalized as byte arrays.
pub struct RawStream {
    pub id: usize,
    pub request: Vec<u8>,
    pub response: Vec<u8>,
}

pub struct HttpStream {
    pub raw_stream: RawStream,
    pub parsed_request: Req,
    pub parsed_response: Resp,
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

impl RawStream {
    pub fn to_list_item(self) -> Option<(ListItem<'static>, HttpStream)> {
        // Parse request
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut parsed_req = httparse::Request::new(&mut headers);
        let res = parsed_req.parse(self.request.as_slice()).unwrap();

        if res.is_partial() {
            return None;
            //panic!("Request is partial {:?}", parsed_req);
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

        // Parse response
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut parsed_resp = httparse::Response::new(&mut headers);
        let res = parsed_resp.parse(self.response.as_slice()).unwrap();

        if res.is_partial() {
            return None;
            //panic!("Request is partial {:?}", parsed_req);
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

        //if !self.filter_stream(&req, &resp) {
        //    return;
        //}

        let body_start = res.unwrap();

        if body_start < self.response.len() {
            resp.body = Some(
                String::from_utf8(self.response[body_start..].to_vec())
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

        let item = ListItem::new(format!(
            "{} {} {} ({} b / {} b)",
            req.version, req.method, req.path, req_len, resp_len
        ));

        let stream = HttpStream {
            raw_stream: self,
            parsed_request: req,
            parsed_response: resp,
        };

        Some((item, stream))
    }
}

impl HttpStream {
    pub fn write_to_text(&self, text: &mut Text) {
        let pr = &self.parsed_request;

        text.extend(Text::raw(format!("{} {}\n", pr.method, pr.path)));

        for header in &pr.headers {
            text.extend(Text::raw(format!("{}: {}\n", header.0, header.1)));
        }

        text.extend(Text::raw("\n"));

        if let Some(ref body) = pr.body {
            text.extend(Text::raw(body.clone()));
        }

        text.extend(Text::raw("\n"));

        let resp = &self.parsed_response;

        text.extend(Text::raw(format!("{} {}", resp.code, resp.version)));

        for header in &resp.headers {
            text.extend(Text::raw(format!("{}: {}\n", header.0, header.1)));
        }

        text.extend(Text::raw("\n"));

        if let Some(ref body) = resp.body {
            text.extend(Text::raw(body.clone()));
        }
    }
}
