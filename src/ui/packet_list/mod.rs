use std::collections::HashMap;

use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, BorderType, Borders, List, ListItem, ListState},
    Frame,
};

use super::{
    http_info::{Req, Resp},
    RawStream,
};

pub struct HttpStream {
    pub raw_stream: RawStream,
    pub parsed_request: Req,
    pub parsed_response: Resp,
}

/// State of the stream list widget.
pub struct StreamListWidget {
    pub streams: Vec<HttpStream>,
    pub stream_items: Vec<ListItem<'static>>,
    pub selected_stream: ListState,
    pub is_selected: bool,
}

impl StreamListWidget {
    pub fn new() -> StreamListWidget {
        StreamListWidget {
            streams: vec![],
            stream_items: vec![],
            selected_stream: ListState::default(),
            is_selected: true,
        }
    }

    pub fn add_stream(&mut self, stream: RawStream) {
        // Parse request
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut parsed_req = httparse::Request::new(&mut headers);
        let res = parsed_req.parse(stream.request.as_slice()).unwrap();

        if res.is_partial() {
            return;
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

        if body_start < stream.request.len() {
            req.body = Some(
                String::from_utf8(stream.request[body_start..].to_vec())
                    .unwrap_or("Encoding error".to_string()),
            );
        }

        // Parse response
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut parsed_resp = httparse::Response::new(&mut headers);
        let res = parsed_resp.parse(stream.response.as_slice()).unwrap();

        if res.is_partial() {
            return;
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

        if body_start < stream.response.len() {
            resp.body = Some(
                String::from_utf8(stream.response[body_start..].to_vec())
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

        self.stream_items.push(item);
        self.streams.push(HttpStream {
            raw_stream: stream,
            parsed_request: req,
            parsed_response: resp,
        });
    }

    /// Draw the stream list widget
    pub fn draw_ui(&mut self, f: &mut Frame, capturing: bool, area: Rect) {
        let border_type = if self.is_selected {
            BorderType::Double
        } else {
            BorderType::Plain
        };

        let title = if capturing {
            Span::styled(
                "HTTP streams (capturing)",
                Style::default().add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw("HTTP streams")
        };

        let list = List::new(self.stream_items.clone())
            .block(
                Block::default()
                    .title(title)
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_type(border_type),
            )
            .highlight_symbol(">>")
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        f.render_stateful_widget(list, area, &mut self.selected_stream);
    }

    /// Move up the list selector in the widget
    pub fn move_up(&mut self) {
        super::move_up(&mut self.selected_stream);
    }

    /// Move down the list selector in the widget
    pub fn move_down(&mut self) {
        super::move_down(&mut self.selected_stream, self.streams.len());
    }

    pub fn handle_key(&mut self, key_code: KeyCode) {
        match key_code {
            KeyCode::Up => {
                self.move_up();
            }
            KeyCode::Down => {
                self.move_down();
            }
            _ => (),
        }
    }
}
