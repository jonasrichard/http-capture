use crossbeam::channel::{Receiver, Sender};

use crate::{capture, ui::RawStream};

pub enum Command {
    StartCapture(String),
    StopCapture,
}

pub fn control_loop(cmd: Receiver<Command>, output: Sender<RawStream>) {
    let mut capture_handler = None;

    while let Ok(command) = cmd.recv() {
        match command {
            Command::StartCapture(interface) => {
                capture_handler = Some(capture::start_capture(interface, output.clone()));
            }
            Command::StopCapture => (),
        }
    }

    drop(cmd);

    if let Some(h) = capture_handler {
        h.join().unwrap();
    }
}
