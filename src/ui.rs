use std::collections::HashMap;
use std::error::Error;
use std::thread;

use crossbeam::channel::{self, Receiver, Sender};
use crossbeam::select;
use crossterm::event::Event::Key;
use crossterm::event::{self, KeyCode};
use pcap::Device;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::{Color, Modifier, Style};
use tui::text::{Span, Spans, Text};
use tui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};
use tui::{backend::Backend, Frame, Terminal};

use crate::capture_control::Command;

const HELP: &str = r#"
C:        Start capture
S:        Stop capture
Q:        Quit
"#;

pub struct RawStream {
    pub id: usize,
    pub request: Vec<u8>,
    pub response: Vec<u8>,
}

impl std::fmt::Debug for RawStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let to = std::cmp::min(self.response.len(), 64);
        let resp = String::from_utf8_lossy(&self.response[0..to]).to_string();

        f.debug_struct("RawStream")
            .field("id", &self.id)
            .field("request", &String::from_utf8(self.request.clone()).unwrap())
            .field("response", &resp)
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

#[derive(PartialEq)]
enum CaptureState {
    Active,
    Inactive,
}

enum SelectedFrame {
    PacketList,
    PacketDetails,
    Help,
    DeviceChooser,
    FilterSetting,
}

pub struct State {
    pub streams: Vec<HttpStream>,
    pub stream_items: Vec<ListItem<'static>>,
    pub input: Receiver<RawStream>,
    pub commands: Sender<Command>,
    pub selected_stream: ListState,
    capture_state: CaptureState,
    selected_frame: SelectedFrame,
    details_scroll: (u16, u16),
    devices: Vec<ListItem<'static>>,
    device_names: Vec<String>,
    selected_device: ListState,
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

        if !self.filter_stream(&req, &resp) {
            return;
        }

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

    fn filter_stream(&self, req: &Req, resp: &Resp) -> bool {
        true
        //if req.path.contains("Cargo") {
        //    return true;
        //}

        //false
    }

    fn move_up(list_state: &mut ListState) {
        let selected = match list_state.selected() {
            Some(p) => {
                if p == 0 {
                    Some(0)
                } else {
                    Some(p - 1)
                }
            }
            None => Some(0),
        };

        list_state.select(selected);
    }

    fn move_down(list_state: &mut ListState, len: usize) {
        let selected = match list_state.selected() {
            Some(p) => {
                if p + 1 == len {
                    Some(p)
                } else {
                    Some(p + 1)
                }
            }
            None => Some(0),
        };

        list_state.select(selected);
    }
}

pub fn new_state(input: Receiver<RawStream>, cmd: Sender<Command>) -> State {
    let devices = Device::list()
        .unwrap_or_default()
        .into_iter()
        .map(|d| {
            let addr = d
                .addresses
                .get(0)
                .map(|a| a.addr.to_string())
                .unwrap_or_default();

            ListItem::new(format!("{} - {}", d.name, addr))
        })
        .collect();

    let mut device_names = vec![];
    for dev in Device::list().unwrap() {
        device_names.push(dev.name);
    }

    State {
        streams: vec![],
        input,
        commands: cmd,
        selected_stream: ListState::default(),
        stream_items: vec![],
        capture_state: CaptureState::Inactive,
        selected_frame: SelectedFrame::PacketList,
        details_scroll: (0, 0),
        devices,
        device_names,
        selected_device: ListState::default(),
    }
}

pub fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut state: State,
) -> Result<(), Box<dyn Error>> {
    let (event_tx, event_rx) = channel::bounded(16);

    let ui_handle = thread::spawn(move || {
        while let Ok(evt) = event::read() {
            if let Err(_) = event_tx.send(evt) {
                break;
            }
        }
    });

    loop {
        terminal.draw(|f| draw_ui(f, &mut state))?;

        select! {
            recv(event_rx) -> event => match event {
                Ok(Key(key)) => {
                    match state.selected_frame {
                        SelectedFrame::PacketList => match key.code {
                            KeyCode::Up => {
                                State::move_up(&mut state.selected_stream);
                                state.details_scroll = (0, 0);
                            }
                            KeyCode::Down => {
                                State::move_down(&mut state.selected_stream, state.streams.len());
                                state.details_scroll = (0, 0);
                            }
                            KeyCode::Tab => {
                                state.selected_frame = SelectedFrame::PacketDetails;
                            }
                            _ => (),
                        },
                        SelectedFrame::PacketDetails => match key.code {
                            KeyCode::Tab => {
                                state.selected_frame = SelectedFrame::PacketList;
                            }
                            KeyCode::Up => {
                                if state.details_scroll.0 > 0 {
                                    state.details_scroll.0 -= 1;
                                }
                            }
                            KeyCode::Down => {
                                state.details_scroll.0 += 1;
                            }
                            KeyCode::PageUp => {
                                if state.details_scroll.0 > 15 {
                                    state.details_scroll.0 -= 15;
                                } else {
                                    state.details_scroll.0 = 0;
                                }
                            }
                            KeyCode::PageDown => {
                                // TODO stop scrolling
                                state.details_scroll.0 += 15;
                            }
                            _ => (),
                        },
                        SelectedFrame::Help => match key.code {
                            KeyCode::Esc => {
                                state.selected_frame = SelectedFrame::PacketList;
                            }
                            _ => (),
                        },
                        SelectedFrame::DeviceChooser => match key.code {
                            KeyCode::Esc => {
                                state.selected_frame = SelectedFrame::PacketList;
                            }
                            KeyCode::Up => {
                                State::move_up(&mut state.selected_device);
                            }
                            KeyCode::Down => {
                                State::move_down(&mut state.selected_device, state.devices.len());
                            }
                            KeyCode::Enter | KeyCode::Char(' ') => {
                                if let Some(selected) = state.selected_device.selected() {
                                    let dev = state.device_names.get(selected).unwrap();

                                    state.selected_frame = SelectedFrame::PacketList;
                                    state.capture_state = CaptureState::Active;
                                    state
                                        .commands
                                        .send(Command::StartCapture(dev.to_string()))?;
                                }
                            }
                            _ => (),
                        },
                        SelectedFrame::FilterSetting => (),
                    }

                    match key.code {
                        KeyCode::Char('q') => {
                            return Ok(());
                        }
                        KeyCode::Char('c') if state.capture_state == CaptureState::Inactive => {
                            state.selected_frame = SelectedFrame::DeviceChooser;
                        }
                        KeyCode::Char('s') if state.capture_state == CaptureState::Active => {
                            state.capture_state = CaptureState::Inactive;
                            state.commands.send(Command::StopCapture)?;
                        }
                        KeyCode::Char('h') => {
                            state.selected_frame = SelectedFrame::Help;
                        }
                        _ => (),
                    }
                },
                Ok(_) => (),
                Err(_) => todo!(),
            },
            recv(state.input) -> stream => match stream {
                Ok(stream) => {
                    state.add_stream(stream);
                },
                Err(e) => panic!("{:?}", e),
            }
        }
    }
}

fn draw_ui<B: Backend>(f: &mut Frame<B>, state: &mut State) {
    let parent_chunk = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(f.size());

    list_streams(f, state, parent_chunk[0]);

    request_response(f, state, parent_chunk[1]);

    match state.selected_frame {
        SelectedFrame::Help => help(f),
        SelectedFrame::DeviceChooser => choose_device(f, state),
        _ => (),
    }
}

fn list_streams<B: Backend>(f: &mut Frame<B>, state: &mut State, area: Rect) {
    let border_type = match state.selected_frame {
        SelectedFrame::PacketList => BorderType::Double,
        _ => BorderType::Plain,
    };

    let title = match state.capture_state {
        CaptureState::Active => Span::styled(
            "HTTP streams (capturing)",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        CaptureState::Inactive => Span::raw("HTTP streams"),
    };

    let list = List::new(state.stream_items.clone())
        .block(
            Block::default()
                .title(Spans::from(title))
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_type(border_type),
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

    let border_type = match state.selected_frame {
        SelectedFrame::PacketDetails => BorderType::Double,
        _ => BorderType::Plain,
    };

    let content = Paragraph::new(text)
        .block(
            Block::default()
                .title("list")
                .borders(Borders::ALL)
                .border_type(border_type),
        )
        .scroll(state.details_scroll);

    f.render_widget(content, area);
}

fn help<B: Backend>(f: &mut Frame<B>) {
    let vertical_margin = (f.size().height - 10) / 2;
    let horizontal_margin = (f.size().width - 30) / 2;
    let rect = Rect::new(horizontal_margin, vertical_margin, 30, 10);

    let help = Paragraph::new(HELP).block(
        Block::default()
            .title("Help")
            .borders(Borders::ALL)
            .border_type(BorderType::Plain),
    );

    f.render_widget(Clear, rect);
    f.render_widget(help, rect);
}

fn choose_device<B: Backend>(f: &mut Frame<B>, state: &mut State) {
    let (width, height) = (70, 30);
    let vertical_margin = (f.size().height - height) / 2;
    let horizontal_margin = (f.size().width - width) / 2;
    let rect = Rect::new(horizontal_margin, vertical_margin, width, height);

    let devices = List::new(state.devices.clone())
        .block(
            Block::default()
                .title("Choose device")
                .borders(Borders::ALL)
                .border_type(BorderType::Plain),
        )
        .highlight_style(Style::default().bg(Color::White));

    f.render_widget(Clear, rect);
    f.render_stateful_widget(devices, rect, &mut state.selected_device);
}
