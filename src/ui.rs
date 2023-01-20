use std::error::Error;
use std::time::Duration;

use crossbeam::channel::{Receiver, Sender};
use crossterm::event::Event::Key;
use crossterm::event::{self, KeyCode};
use tui::layout::{Constraint, Direction, Layout, Rect};
use tui::style::{Modifier, Style};
use tui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};
use tui::{backend::Backend, Frame, Terminal};

use crate::capture_control::Command;

pub struct HttpStream {
    pub id: u16,
    pub request: Vec<u8>,
    pub response: Vec<u8>,
}

pub struct State {
    pub streams: Vec<HttpStream>,
    pub stream_items: Vec<ListItem<'static>>,
    pub input: Receiver<HttpStream>,
    pub commands: Sender<Command>,
    pub selected_stream: ListState,
}

impl State {
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

pub fn new_state(input: Receiver<HttpStream>, cmd: Sender<Command>) -> State {
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
                    KeyCode::Char('x') => {
                        state.streams.push(HttpStream {
                            id: 1,
                            request: b"Req".to_vec(),
                            response: b"".to_vec(),
                        });
                        state.stream_items.push(ListItem::new("Req"));
                        need_redraw = true;
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
            state.streams.push(stream);
            state.stream_items.push(ListItem::new("Req"));
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
    let text = format!("Test {:?}", state.selected_stream.selected());
    let content = Paragraph::new(text).block(
        Block::default()
            .title("list")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded),
    );

    f.render_widget(content, area);
}
