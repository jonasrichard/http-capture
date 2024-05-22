use chrono::Local;
use crossbeam::channel;
use crossterm::{
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use log::{info, Level, LevelFilter, Log};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    fs::File,
    io::{stdout, Result, Write},
    sync::Mutex,
    thread,
};

mod capture;
mod capture_control;
mod stream;
pub mod ui;

struct FileLog {
    file: Mutex<File>,
}

fn main() -> Result<()> {
    setup_logger();

    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    let (http_tx, http_rx) = channel::bounded(32);
    let (cmd_tx, cmd_rx) = channel::bounded(4);

    let state = ui::State::new(http_rx, cmd_tx);

    let capture_handle = thread::spawn(move || {
        capture_control::control_loop(cmd_rx, http_tx);
    });

    info!("Starting main ui");

    let result = ui::run_app(&mut terminal, state);

    stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;

    if let Err(e) = result {
        eprintln!("{e}");
    }

    capture_handle.join().unwrap();

    Ok(())
}

impl Log for FileLog {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let mut f = self.file.lock().unwrap();

            writeln!(
                f,
                "{} - {} - {:?}:{:?} - {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.file(),
                record.line(),
                record.args()
            )
            .unwrap();
        }
    }

    fn flush(&self) {}
}

impl FileLog {
    fn new() -> FileLog {
        let log_file = File::create("out.log").expect("Cannot create out.log");

        FileLog {
            file: Mutex::new(log_file),
        }
    }
}

fn setup_logger() {
    log::set_boxed_logger(Box::new(FileLog::new())).expect("Failed to set logger");
    log::set_max_level(LevelFilter::Info);
}
