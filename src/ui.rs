use std::error::Error;
use std::sync::mpsc;
use std::time::Duration;

use crossterm::event::Event::Key;
use crossterm::event::{self, KeyCode};
use tui::layout::{Constraint, Direction, Layout};
use tui::widgets::{Block, BorderType, Borders};
use tui::{backend::Backend, Frame, Terminal};

use crate::capture::Command;

pub struct HttpStream {
    pub id: u16,
    pub request: Vec<u8>,
    pub response: Vec<u8>,
}

pub struct State {
    pub streams: Vec<HttpStream>,
    pub input: mpsc::Receiver<HttpStream>,
    pub commands: mpsc::Sender<Command>,
}

pub fn new_state(input: mpsc::Receiver<HttpStream>, cmd: mpsc::Sender<Command>) -> State {
    State {
        streams: vec![],
        input,
        commands: cmd,
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
                        drop(state.commands);

                        return Ok(());
                    }
                    KeyCode::Char('c') => {
                        // TODO display a label that we are capturing, maybe we need capture mode
                        // in the state
                        state
                            .commands
                            .send(Command::StartCapture("lo0".to_string()))?;
                    }
                    _ => (),
                }
            }
        }

        if let Ok(stream) = state.input.recv_timeout(Duration::from_millis(100)) {
            state.streams.push(stream);
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

    let traffic_block = Block::default()
        .title("HTTP traffic")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(traffic_block, parent_chunk[0]);

    let detail_block = Block::default()
        .title("Request/response details")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(detail_block, parent_chunk[1]);
}
