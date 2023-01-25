use crossbeam::channel::bounded;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{env, error::Error, thread};
use tui::{backend::CrosstermBackend, Terminal};

mod capture;
mod capture_control;
mod ui;

fn start_ui() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    execute!(std::io::stdout(), EnterAlternateScreen, EnableMouseCapture,)?;

    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let (http_tx, http_rx) = bounded(5);
    let (cmd_tx, cmd_rx) = bounded(5);

    let state = ui::new_state(http_rx, cmd_tx);

    let capture_handle = thread::spawn(move || {
        capture_control::control_loop(cmd_rx, http_tx);
    });

    let result = ui::run_app(&mut terminal, state);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;

    if let Err(e) = result {
        eprintln!("{}", e);
    }

    capture_handle.join().unwrap();

    Ok(())
}

fn start_cli() {
    let (http_tx, http_rx) = bounded(5);
    let (cmd_tx, cmd_rx) = bounded(5);

    let capture_handle = thread::spawn(move || {
        capture_control::control_loop(cmd_rx, http_tx);
    });

    cmd_tx
        .send(capture_control::Command::StartCapture("lo0".to_string()))
        .unwrap();

    while let Ok(stream) = http_rx.recv() {
        println!("{:?}", stream);
    }

    cmd_tx.send(capture_control::Command::StopCapture).unwrap();

    capture_handle.join().unwrap();
}

fn main() {
    if env::args().nth(1) == Some("--dev".to_string()) {
        start_cli();
    } else {
        start_ui().unwrap();
    }
}
