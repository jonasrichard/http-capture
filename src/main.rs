use crossbeam::channel::bounded;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{error::Error, thread};
use tui::{backend::CrosstermBackend, Terminal};

mod capture;
mod capture_control;
mod ui;

fn main() -> Result<(), Box<dyn Error>> {
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
