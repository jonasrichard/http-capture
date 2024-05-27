use crossbeam::channel::{self, Receiver, Sender};
use log::info;

use crate::{capture, ui::stream::HttpStream};

#[derive(Debug)]
pub enum Command {
    StartCapture(String),
    StopCapture,
}

pub fn control_loop(cmd: Receiver<Command>, output: Sender<HttpStream>) {
    let mut command_sender = None;

    while let Ok(command) = cmd.recv() {
        info!("Command arrived: {command:?}");

        match command {
            Command::StartCapture(interface) => {
                command_sender = Some(start_capture(interface, output.clone()));
            }
            Command::StopCapture => {
                if let Some(ref tx) = command_sender {
                    tx.send(capture::Command::StopCapture).unwrap();
                }
            }
        }
    }
}

pub fn start_capture(interface: String, packet_tx: Sender<HttpStream>) -> Sender<capture::Command> {
    let (command_tx, command_rx) = channel::bounded(5);

    capture::start_capture(interface, 9000, packet_tx, command_rx);

    command_tx
}
