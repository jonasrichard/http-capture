use std::collections::HashMap;

use crossterm::event::KeyCode;
use ratatui::{
    layout::Rect,
    text::Text,
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use super::packet_list::HttpStream;

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

pub struct StreamInfoWidget {
    selected_stream: Option<HttpStream>,
    details_scroll: (u16, u16),
    pub is_selected: bool,
}

impl StreamInfoWidget {
    pub fn new() -> StreamInfoWidget {
        StreamInfoWidget {
            selected_stream: None,
            details_scroll: (0, 0),
            is_selected: false,
        }
    }

    pub fn reset_scroll(&mut self) {
        self.details_scroll = (0, 0);
    }

    pub fn scroll_up(&mut self) {
        if self.details_scroll.0 > 0 {
            self.details_scroll.0 -= 1;
        }
    }

    pub fn scroll_down(&mut self) {
        self.details_scroll.0 += 1;
    }

    pub fn scroll_page_up(&mut self) {
        if self.details_scroll.0 > 15 {
            self.details_scroll.0 -= 15;
        } else {
            self.details_scroll.0 = 0;
        }
    }

    pub fn scroll_page_down(&mut self) {
        self.details_scroll.0 += 15;
    }

    pub fn draw_ui(&mut self, f: &mut Frame, area: Rect) {
        let mut text = Text::from("");

        if let Some(selected) = &self.selected_stream {
            let pr = &selected.parsed_request;

            text.extend(Text::raw(format!("{} {}\n", pr.method, pr.path)));

            for header in &pr.headers {
                text.extend(Text::raw(format!("{}: {}\n", header.0, header.1)));
            }

            text.extend(Text::raw("\n"));

            if let Some(ref body) = pr.body {
                text.extend(Text::raw(body));
            }

            text.extend(Text::raw("\n"));

            let resp = &selected.parsed_response;

            text.extend(Text::raw(format!("{} {}", resp.code, resp.version)));

            for header in &resp.headers {
                text.extend(Text::raw(format!("{}: {}\n", header.0, header.1)));
            }

            text.extend(Text::raw("\n"));

            if let Some(ref body) = resp.body {
                text.extend(Text::raw(body));
            }
        }

        let border_type = if self.is_selected {
            BorderType::Double
        } else {
            BorderType::Plain
        };

        let content = Paragraph::new(text)
            .block(
                Block::default()
                    .title("list")
                    .borders(Borders::ALL)
                    .border_type(border_type),
            )
            .scroll(self.details_scroll);

        f.render_widget(content, area);
    }

    pub fn handle_key(&mut self, key_code: KeyCode) {
        match key_code {
            KeyCode::Tab => {
                self.is_selected = false;
            }
            KeyCode::Up => self.scroll_up(),
            KeyCode::Down => self.scroll_down(),
            KeyCode::PageUp => self.scroll_page_up(),
            KeyCode::PageDown => self.scroll_page_down(),
            _ => (),
        }
    }
}
