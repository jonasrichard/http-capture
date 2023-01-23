use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

use crossbeam::channel::{Receiver, Sender};
use crossterm::event::Event::Key;
use crossterm::event::{self, KeyCode};
use tui::layout::{Constraint, Direction, Layout, Rect};
use tui::style::{Modifier, Style};
use tui::text::{Span, Spans, Text};
use tui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};
use tui::{backend::Backend, Frame, Terminal};

use crate::capture_control::Command;

pub struct RawStream {
    pub id: u16,
    pub request: Vec<u8>,
    pub response: Vec<u8>,
}

impl std::fmt::Debug for RawStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawStream")
            .field("id", &self.id)
            .field("request", &String::from_utf8(self.request.clone()).unwrap())
            .field(
                "response",
                &String::from_utf8(self.response.clone()).unwrap(),
            )
            .finish()
    }
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

pub struct HttpStream {
    pub raw_stream: RawStream,
    pub parsed_request: Req,
    pub parsed_response: Resp,
}

pub struct State {
    pub streams: Vec<HttpStream>,
    pub stream_items: Vec<ListItem<'static>>,
    pub input: Receiver<RawStream>,
    pub commands: Sender<Command>,
    pub selected_stream: ListState,
}

impl State {
    fn add_stream(&mut self, stream: RawStream) {
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

        let body_start = res.unwrap();

        if body_start < stream.response.len() {
            resp.body = Some(
                String::from_utf8(stream.response[body_start..].to_vec())
                    .unwrap_or("Encoding error".to_string()),
            );
        }

        let item = ListItem::new(format!("{} {} {}", req.version, req.method, req.path));

        self.stream_items.push(item);
        self.streams.push(HttpStream {
            raw_stream: stream,
            parsed_request: req,
            parsed_response: resp,
        });
    }

    fn move_up(&mut self) {
        let selected = match self.selected_stream.selected() {
            Some(p) => {
                if p == 0 {
                    Some(0)
                } else {
                    Some(p - 1)
                }
            }
            None => Some(0),
        };

        self.selected_stream.select(selected);
    }

    fn move_down(&mut self) {
        let selected = match self.selected_stream.selected() {
            Some(p) => {
                if p + 1 == self.streams.len() {
                    Some(p)
                } else {
                    Some(p + 1)
                }
            }
            None => Some(0),
        };

        self.selected_stream.select(selected);
    }
}

pub fn new_state(input: Receiver<RawStream>, cmd: Sender<Command>) -> State {
    State {
        streams: vec![],
        input,
        commands: cmd,
        selected_stream: ListState::default(),
        stream_items: vec![],
    }
}

pub fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut state: State,
) -> Result<(), Box<dyn Error>> {
    terminal.draw(|f| draw_ui(f, &mut state))?;

    loop {
        let mut need_redraw = false;

        if event::poll(Duration::from_millis(100))? {
            if let Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => {
                        //drop(state.commands);

                        return Ok(());
                    }
                    KeyCode::Char('c') => {
                        // TODO display a label that we are capturing, maybe we need capture mode
                        // in the state
                        state
                            .commands
                            .send(Command::StartCapture("lo0".to_string()))?;
                    }
                    KeyCode::Up => {
                        state.move_up();
                        need_redraw = true;
                    }
                    KeyCode::Down => {
                        state.move_down();
                        need_redraw = true;
                    }
                    _ => (),
                }
            }
        }

        if let Ok(stream) = state.input.recv_timeout(Duration::from_millis(100)) {
            state.add_stream(stream);
            need_redraw = true;
        }

        if need_redraw {
            terminal.draw(|f| draw_ui(f, &mut state))?;
        }
    }
}

fn draw_ui<B: Backend>(f: &mut Frame<B>, state: &mut State) {
    let parent_chunk = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(f.size());

    //let traffic_block = Block::default()
    //    .title("HTTP traffic")
    //    .borders(Borders::ALL)
    //    .border_type(BorderType::Rounded);
    //f.render_widget(traffic_block, parent_chunk[0]);

    list_streams(f, state, parent_chunk[0]);

    //let detail_block = Block::default()
    //    .title("Request/response details")
    //    .borders(Borders::ALL)
    //    .border_type(BorderType::Rounded);
    //f.render_widget(detail_block, parent_chunk[1]);

    request_response(f, state, parent_chunk[1]);
}

fn list_streams<B: Backend>(f: &mut Frame<B>, state: &mut State, area: Rect) {
    let list = List::new(state.stream_items.clone())
        .block(
            Block::default()
                .title("list")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        )
        .highlight_symbol(">>")
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    f.render_stateful_widget(list, area, &mut state.selected_stream);
}

fn request_response<B: Backend>(f: &mut Frame<B>, state: &mut State, area: Rect) {
    let mut text = Text::from("");

    if let Some(selected) = state.selected_stream.selected() {
        if let Some(s) = &state.streams.get(selected) {
            let pr = &s.parsed_request;

            text.extend(Text::raw(format!("{} {}\n", pr.method, pr.path)));

            for header in &pr.headers {
                text.extend(Text::raw(format!("{}: {}\n", header.0, header.1)));
            }

            text.extend(Text::raw("\n"));

            if let Some(ref body) = pr.body {
                text.extend(Text::raw(body));
            }

            text.extend(Text::raw("\n"));

            let resp = &s.parsed_response;

            text.extend(Text::raw(format!("{} {}", resp.code, resp.version)));

            for header in &resp.headers {
                text.extend(Text::raw(format!("{}: {}\n", header.0, header.1)));
            }

            text.extend(Text::raw("\n"));

            if let Some(ref body) = resp.body {
                text.extend(Text::raw(body));
            }
        }
    }

    let content = Paragraph::new(text).block(
        Block::default()
            .title("list")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded),
    );

    f.render_widget(content, area);
}
