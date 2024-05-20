use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Clear, List},
    Frame,
};

use super::State;

pub fn choose_device(f: &mut Frame, state: &mut State) {
    let (width, height) = (70, 30);
    let vertical_margin = (f.size().height - height) / 2;
    let horizontal_margin = (f.size().width - width) / 2;
    let rect = Rect::new(horizontal_margin, vertical_margin, width, height);

    let devices = List::new(state.devices.clone())
        .block(
            Block::default()
                .title("Choose device")
                .borders(Borders::ALL)
                .border_type(BorderType::Plain),
        )
        .highlight_style(Style::default().bg(Color::White));

    f.render_widget(Clear, rect);
    f.render_stateful_widget(devices, rect, &mut state.selected_device);
}
