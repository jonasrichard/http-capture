mod common;
mod devices;
mod http_info;
mod packet_list;

use crossbeam::{
    channel::{self, Receiver, Sender},
    select,
};
use crossterm::event::{
    self,
    Event::{self, Key},
    KeyCode, KeyEvent,
};
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, BorderType, Borders, Clear, ListState, Paragraph},
    Frame, Terminal,
};
use std::{error::Error, thread};

use crate::capture_control::Command;

use self::{devices::DevicesWidget, http_info::StreamInfoWidget, packet_list::StreamListWidget};

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
            .field(
                "request",
                &String::from_utf8(self.request.clone()).unwrap_or("Non UTF-8 data".to_string()),
            )
            .field("response", &resp)
            .finish()
    }
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
    pub input: Receiver<RawStream>,
    pub commands: Sender<Command>,
    capture_state: CaptureState,
    selected_frame: SelectedFrame,
    pub stream_list_widget: packet_list::StreamListWidget,
    pub stream_info_widget: http_info::StreamInfoWidget,
    pub devices_widget: devices::DevicesWidget,
    details_scroll: (u16, u16),
}

impl State {
    pub fn new(input: Receiver<RawStream>, cmd: Sender<Command>) -> State {
        State {
            input,
            commands: cmd,
            capture_state: CaptureState::Inactive,
            selected_frame: SelectedFrame::PacketList,
            details_scroll: (0, 0),
            stream_list_widget: StreamListWidget::new(),
            stream_info_widget: StreamInfoWidget::new(),
            devices_widget: DevicesWidget::new(),
        }
    }

    fn handle_event(&mut self, event: Event) {
        if let Key(key) = event {
            // Global key shortcuts
            match key.code {
                KeyCode::Char('q') => {}
                KeyCode::Char('c') => {
                    if self.capture_state == CaptureState::Inactive {
                        self.selected_frame = SelectedFrame::DeviceChooser;
                        return;
                    }
                }
                KeyCode::Char('s') => {
                    if self.capture_state == CaptureState::Active {
                        self.capture_state = CaptureState::Inactive;
                        self.commands.send(Command::StopCapture).unwrap();
                    }
                }
                KeyCode::Char('h') => {
                    self.selected_frame = SelectedFrame::Help;
                }
                _ => {}
            }

            match self.selected_frame {
                SelectedFrame::PacketList => self.handle_key_stream_list(key.code),
                SelectedFrame::PacketDetails => self.handle_key_http_info(key.code),
                SelectedFrame::Help => todo!(),
                SelectedFrame::DeviceChooser => todo!(),
                _ => {}
            }
        }
    }

    fn handle_key_stream_list(&mut self, key_code: KeyCode) {
        self.stream_list_widget.handle_key(key_code);

        match key_code {
            KeyCode::Up => {
                self.stream_info_widget.reset_scroll();
            }
            KeyCode::Down => {
                self.stream_info_widget.reset_scroll();
            }
            KeyCode::Tab => {
                self.selected_frame = SelectedFrame::PacketDetails;
                self.stream_info_widget.is_selected = true;
            }
            KeyCode::Char('q') => {
                // TODO quit, stop capturing, join to ui_handle
            }
            _ => {}
        }
    }

    fn handle_key_http_info(&mut self, key_code: KeyCode) {
        self.stream_info_widget.handle_key(key_code);

        match key_code {
            KeyCode::Tab => {
                self.selected_frame = SelectedFrame::PacketList;
                self.stream_list_widget.is_selected = true;
            }
            _ => {}
        }
    }

    fn handle_key_device_widget(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.selected_frame = SelectedFrame::PacketList;
            }
            KeyCode::Up => {
                move_up(&mut self.devices_widget.selected_device);
            }
            KeyCode::Down => {
                move_down(
                    &mut self.devices_widget.selected_device,
                    self.devices_widget.devices.len(),
                );
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(selected) = self.devices_widget.selected_device.selected() {
                    let dev = self.devices_widget.device_names.get(selected).unwrap();

                    self.selected_frame = SelectedFrame::PacketList;
                    self.capture_state = CaptureState::Active;
                    // TODO in the layout we can have some minibuffer for the messages
                    self.commands
                        .send(Command::StartCapture(dev.to_string()))
                        .unwrap();
                }
            }
            _ => (),
        }
    }

    fn draw_ui(&mut self, f: &mut Frame) {
        let parent_chunk = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(f.size());

        self.stream_list_widget.draw_ui(
            f,
            self.capture_state == CaptureState::Active,
            parent_chunk[0],
        );

        self.stream_info_widget.draw_ui(f, parent_chunk[1]);

        match self.selected_frame {
            SelectedFrame::Help => help(f),
            SelectedFrame::DeviceChooser => self.devices_widget.draw_ui(f),
            _ => (),
        }
    }
}

pub fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut state: State,
) -> Result<(), Box<dyn Error>> {
    let (event_tx, event_rx) = channel::bounded(5);

    let _ui_handle = thread::spawn(move || {
        while let Ok(evt) = event::read() {
            if event_tx.send(evt).is_err() {
                break;
            }
        }
    });

    loop {
        terminal.draw(|f| state.draw_ui(f));

        select! {
            recv(event_rx) -> event => match event {
                Ok(evt) => {
                    state.handle_event(evt);
                }
                Err(_) => {}
            },
            recv(state.input) -> stream => match stream {
                Ok(stream) => {
                    state.stream_list_widget.add_stream(stream);
                },
                Err(e) => panic!("{:?}", e),
            }
        }
    }
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

fn help(f: &mut Frame) {
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

//fn filter_stream(f: &mut Frame, _state: &mut State) {
//    let (width, height) = (70, 30);
//    let vertical_margin = (f.size().height - height) / 2;
//    let horizontal_margin = (f.size().width - width) / 2;
//    let _rect = Rect::new(horizontal_margin, vertical_margin, width, height);
//}
