use crossbeam::channel;
use crossterm::{
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io::{stdout, Result},
    thread,
};

mod capture;
mod capture_control;
mod ui;

fn main() -> Result<()> {
    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    let (http_tx, http_rx) = channel::bounded(5);
    let (cmd_tx, cmd_rx) = channel::bounded(5);

    let state = ui::new_state(http_rx, cmd_tx);

    let capture_handle = thread::spawn(move || {
        capture_control::control_loop(cmd_rx, http_tx);
    });

    let result = ui::run_app(&mut terminal, state);

    stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;

    if let Err(e) = result {
        eprintln!("{e}");
    }

    capture_handle.join().unwrap();

    Ok(())
}

//fn start_ui() -> Result<(), Box<dyn Error>> {
//    enable_raw_mode()?;
//    execute!(std::io::stdout(), EnterAlternateScreen, EnableMouseCapture,)?;
//
//    let backend = CrosstermBackend::new(std::io::stdout());
//    let mut terminal = Terminal::new(backend)?;
//
//    let (http_tx, http_rx) = bounded(5);
//    let (cmd_tx, cmd_rx) = bounded(5);
//
//    let state = ui::new_state(http_rx, cmd_tx);
//
//    let capture_handle = thread::spawn(move || {
//        capture_control::control_loop(cmd_rx, http_tx);
//    });
//
//    let result = ui::run_app(&mut terminal, state);
//
//    disable_raw_mode()?;
//    execute!(
//        terminal.backend_mut(),
//        LeaveAlternateScreen,
//        DisableMouseCapture
//    )?;
//
//    if let Err(e) = result {
//        eprintln!("{}", e);
//    }
//
//    capture_handle.join().unwrap();
//
//    Ok(())
//}
//
//fn start_cli(interface: String) {
//    let (http_tx, http_rx) = bounded(5);
//    let (cmd_tx, cmd_rx) = bounded(5);
//
//    let capture_handle = thread::spawn(move || {
//        capture_control::control_loop(cmd_rx, http_tx);
//    });
//
//    cmd_tx
//        .send(capture_control::Command::StartCapture(interface))
//        .unwrap();
//
//    while let Ok(stream) = http_rx.recv() {
//        println!("{:?}", stream);
//    }
//
//    cmd_tx.send(capture_control::Command::StopCapture).unwrap();
//
//    capture_handle.join().unwrap();
//}
//
//fn main() {
//    if env::args().nth(1) == Some("--dev".to_string()) {
//        start_cli(env::args().nth(2).unwrap_or("lo0".to_string()));
//    } else {
//        start_ui().unwrap();
//    }
//}
