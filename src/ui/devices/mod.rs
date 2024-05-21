use pcap::Device;
use ratatui::{
    layout::{Alignment, Constraint, Layout},
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Padding, Paragraph},
    Frame,
};

use super::common;

pub struct DevicesWidget {
    pub devices: Vec<ListItem<'static>>,
    pub device_names: Vec<String>,
    pub selected_device: ListState,
}

impl DevicesWidget {
    pub fn new() -> DevicesWidget {
        let devices = Device::list()
            .unwrap_or_default()
            .into_iter()
            .map(|d| {
                let addr = d
                    .addresses
                    .first()
                    .map(|a| a.addr.to_string())
                    .unwrap_or_default();

                ListItem::new(format!("{} - {}", d.name, addr))
            })
            .collect();

        let mut device_names = vec![];

        for dev in Device::list().unwrap() {
            device_names.push(dev.name);
        }

        DevicesWidget {
            devices,
            device_names,
            selected_device: ListState::default(),
        }
    }

    pub fn draw_ui(&mut self, f: &mut Frame) {
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
}
