mod common;
pub mod stream;

use crossbeam::{
    channel::{self, Receiver, Sender},
    select,
};
use crossterm::event::{
    self,
    Event::{self, Key},
    KeyCode,
};
use log::error;
use pcap::Device;
use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Text},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Row,
        Table, TableState,
    },
    Frame, Terminal,
};
use std::{error::Error, fs::File, io::BufWriter, thread};

use crate::capture_control::Command;

use self::stream::{HttpStream, RawStream};

const HELP: &str = r#"
C:        Start capture
S:        Stop capture
Q:        Quit
"#;

#[derive(Copy, Clone, PartialEq)]
pub enum CaptureState {
    Active,
    Inactive,
}

/// Tells which window is currently on focus.
#[derive(Copy, Clone, PartialEq)]
pub enum SelectedWindow {
    PacketList,
    PacketDetails,
    Help,
    DeviceChooser,
    FilterSetting,
}

pub struct State {
    input: Receiver<RawStream>,
    commands: Sender<Command>,
    capture_state: CaptureState,
    selected_window: SelectedWindow,
    streams: Vec<HttpStream>,
    stream_items: Vec<Row<'static>>,
    selected_stream: TableState,
    details_scroll: (u16, u16),
    devices: Vec<ListItem<'static>>,
    device_names: Vec<String>,
    selected_device: ListState,
    status_line: String,
}

impl State {
    pub fn new(input: Receiver<RawStream>, cmd: Sender<Command>) -> State {
        let (devices, device_names) = device_list();

        State {
            input,
            commands: cmd,
            capture_state: CaptureState::Inactive,
            selected_window: SelectedWindow::PacketList,
            streams: vec![],
            stream_items: vec![],
            selected_stream: TableState::default(),
            details_scroll: (0, 0),
            devices,
            device_names,
            selected_device: ListState::default(),
            status_line: String::from(""),
        }
    }

    fn handle_event(&mut self, event: Event) -> bool {
        if let Key(key) = event {
            // Global key shortcuts
            match key.code {
                KeyCode::Char('q') => {
                    if self.capture_state == CaptureState::Active {
                        self.commands.send(Command::StopCapture).unwrap();
                    }

                    return false;
                }
                KeyCode::Char('c') => {
                    if self.capture_state == CaptureState::Inactive {
                        self.set_selected_window(SelectedWindow::DeviceChooser);

                        return true;
                    }
                }
                KeyCode::Char('s') => {
                    if self.capture_state == CaptureState::Active {
                        self.set_capture_state(CaptureState::Inactive);
                        self.commands.send(Command::StopCapture).unwrap();
                        self.status_line = String::from("Stop capturing");
                    }
                }
                KeyCode::Char('h') => {
                    self.set_selected_window(SelectedWindow::Help);
                }
                _ => {}
            }

            match self.selected_window {
                SelectedWindow::PacketList => self.handle_key_stream_list(key.code),
                SelectedWindow::PacketDetails => self.handle_key_http_info(key.code),
                SelectedWindow::Help => todo!(),
                SelectedWindow::DeviceChooser => self.handle_key_device_widget(key.code),
                _ => {}
            }
        }

        true
    }

    fn handle_key_stream_list(&mut self, key_code: KeyCode) {
        match key_code {
            KeyCode::Up => {
                table_move_up(&mut self.selected_stream);
                self.reset_scroll();
            }
            KeyCode::Down => {
                table_move_down(&mut self.selected_stream, self.stream_items.len());
                self.reset_scroll();
            }
            KeyCode::Char('p') => {
                self.save_http_stream("http-stream.txt");
                self.status_line = format!("HTTP stream saved").to_string();
            }
            KeyCode::Tab => self.set_selected_window(SelectedWindow::PacketDetails),
            KeyCode::Char('q') => {
                // TODO quit, stop capturing, join to ui_handle
            }
            _ => {}
        }
    }

    fn handle_key_http_info(&mut self, key_code: KeyCode) {
        match key_code {
            KeyCode::Tab => self.set_selected_window(SelectedWindow::PacketList),
            KeyCode::Up => self.scroll_up(),
            KeyCode::Down => self.scroll_down(),
            KeyCode::PageUp => self.scroll_page_up(),
            KeyCode::PageDown => self.scroll_page_down(),
            _ => {}
        }
    }

    fn handle_key_device_widget(&mut self, key_code: KeyCode) {
        match key_code {
            KeyCode::Esc => {
                self.set_selected_window(SelectedWindow::PacketList);
            }
            KeyCode::Up => {
                list_move_up(&mut self.selected_device);
            }
            KeyCode::Down => {
                list_move_down(&mut self.selected_device, self.devices.len());
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(dev) = self.get_selected_device_name() {
                    self.set_selected_window(SelectedWindow::PacketList);
                    self.set_capture_state(CaptureState::Active);
                    // TODO in the layout we can have some minibuffer for the messages
                    self.commands
                        .send(Command::StartCapture(dev.to_string()))
                        .unwrap();
                    self.status_line = format!("Start capturing on device {}", dev);
                }
            }
            _ => (),
        }
    }

    fn draw_ui(&mut self, f: &mut Frame) {
        let parent_chunk = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
                Constraint::Length(1),
            ])
            .split(f.size());

        self.stream_list_draw_ui(f, parent_chunk[0]);
        self.stream_info_draw_ui(f, parent_chunk[1]);

        let status = Paragraph::new(Text::styled(self.status_line.clone(), Color::White));

        f.render_widget(status, parent_chunk[2]);

        match self.selected_window {
            SelectedWindow::Help => help(f),
            SelectedWindow::DeviceChooser => self.devices_draw_ui(f),
            _ => (),
        }
    }

    /// Draw the stream list widget
    pub fn stream_list_draw_ui(&mut self, f: &mut Frame, area: Rect) {
        let border_type = if self.selected_window == SelectedWindow::PacketList {
            BorderType::Double
        } else {
            BorderType::Plain
        };

        let title = if self.capture_state == CaptureState::Active {
            Span::styled(
                "HTTP streams (capturing)",
                Style::default().add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw("HTTP streams")
        };

        let stream_list = Table::new(
            self.stream_items.clone(),
            vec![
                Constraint::Length(12),
                Constraint::Length(20),
                Constraint::Length(20),
                Constraint::Fill(1),
            ],
        )
        .header(
            Row::new(vec!["Timestamp", "Source", "Destination", "Path"])
                .style(Style::new().fg(Color::White).add_modifier(Modifier::BOLD)),
        )
        .block(
            Block::default()
                .title(title)
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_type(border_type),
        )
        .highlight_style(
            Style::new()
                .bg(Color::LightGreen)
                .fg(Color::Black)
                .add_modifier(Modifier::ITALIC),
        )
        .highlight_symbol("> ")
        .highlight_spacing(ratatui::widgets::HighlightSpacing::Always);

        f.render_stateful_widget(stream_list, area, &mut self.selected_stream);
    }

    pub fn stream_info_draw_ui(&mut self, f: &mut Frame, area: Rect) {
        let mut text = Text::default();

        if let Some(selected) = &self.selected_stream.selected() {
            if let Some(selected_stream) = self.streams.get(*selected) {
                selected_stream.write_to_text(&mut text);
            }
        }

        let border_type = if self.selected_window == SelectedWindow::PacketDetails {
            BorderType::Double
        } else {
            BorderType::Plain
        };

        let content = Paragraph::new(text)
            .block(
                Block::default()
                    .title("HTTP Stream Info")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_type(border_type),
            )
            .scroll(self.details_scroll);

        f.render_widget(content, area);
    }

    pub fn devices_draw_ui(&mut self, f: &mut Frame) {
        let rect = common::center_rect(f, 70, 30);

        let dialog = Block::default().borders(Borders::ALL);

        let dialog_layout = Layout::default()
            .constraints(vec![Constraint::Min(2), Constraint::Percentage(100)])
            .split(dialog.inner(rect));

        let devices = List::new(self.devices.clone())
            .block(
                Block::default()
                    .title("Choose device")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_type(BorderType::Plain)
                    .padding(Padding::uniform(1)),
            )
            .highlight_style(Style::default().bg(Color::White));

        f.render_widget(Clear, rect);
        f.render_widget(dialog, rect);
        f.render_widget(
            Paragraph::new("Choose a device and press Enter"),
            dialog_layout[0],
        );
        f.render_stateful_widget(devices, dialog_layout[1], &mut self.selected_device);
    }

    fn set_selected_window(&mut self, s: SelectedWindow) {
        self.selected_window = s;
    }

    fn set_capture_state(&mut self, c: CaptureState) {
        self.capture_state = c;
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

    /// Add a new RawStream to the UI and convert it to a HttpStream.
    pub fn add_stream(&mut self, stream: RawStream) {
        if let Ok(http_stream) = HttpStream::try_from(stream) {
            self.stream_items.push(http_stream.clone().into());
            self.streams.push(http_stream);
        }
    }

    pub fn save_http_stream(&mut self, file_name: &str) {
        let f = File::create(file_name).unwrap();
        let writer = BufWriter::new(f);

        if let Some(selected) = &self.selected_stream.selected() {
            if let Some(selected_stream) = self.streams.get(*selected) {
                selected_stream.write_to_file(writer);
            }
        }
    }

    pub fn get_selected_device_name(&self) -> Option<String> {
        if let Some(selected) = self.selected_device.selected() {
            let dev = self.device_names.get(selected).unwrap();

            Some(dev.to_string())
        } else {
            None
        }
    }
}

fn device_list() -> (Vec<ListItem<'static>>, Vec<String>) {
    let devices = Device::list()
        .unwrap_or_default()
        .into_iter()
        .map(|d| {
            let addr = d
                .addresses
                .first()
                .map(|a| a.addr.to_string())
                .unwrap_or_default();

            ListItem::new(format!("{:7} - {}", d.name, addr))
        })
        .collect();

    let mut device_names = vec![];

    for dev in Device::list().unwrap() {
        device_names.push(dev.name);
    }

    (devices, device_names)
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

    'main: loop {
        if let Err(e) = terminal.draw(|f| state.draw_ui(f)) {
            error!("Error during draw_ui {e:?}");
        }

        select! {
            recv(event_rx) -> event => match event {
                Ok(evt) => {
                    if !state.handle_event(evt) {
                        break 'main Ok(())
                    }
                }
                Err(e) => {
                    error!("Error in run_app recv: {e:?}");
                }
            },
            recv(state.input) -> stream => match stream {
                Ok(stream) => {
                    state.add_stream(stream);
                },
                Err(e) => {
                    error!("Error in run_app loop: {e:?}");

                    return Ok(())
                }
            }
        }
    }
}

fn list_move_up(list_state: &mut ListState) {
    list_state.select(move_up(list_state.selected()));
}

fn list_move_down(list_state: &mut ListState, len: usize) {
    list_state.select(move_down(list_state.selected(), len));
}

fn table_move_up(tbl_state: &mut TableState) {
    tbl_state.select(move_up(tbl_state.selected()));
}

fn table_move_down(tbl_state: &mut TableState, len: usize) {
    tbl_state.select(move_down(tbl_state.selected(), len));
}

fn move_up(pos: Option<usize>) -> Option<usize> {
    if let Some(p) = pos {
        if p == 0 {
            Some(0)
        } else {
            Some(p - 1)
        }
    } else {
        Some(0)
    }
}

fn move_down(pos: Option<usize>, len: usize) -> Option<usize> {
    if let Some(p) = pos {
        if p + 1 == len {
            Some(p)
        } else {
            Some(p + 1)
        }
    } else {
        Some(0)
    }
}

fn help(f: &mut Frame) {
    let rect = common::center_rect(f, 30, 10);

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
