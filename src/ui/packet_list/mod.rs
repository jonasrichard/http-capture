use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, BorderType, Borders, List},
    Frame,
};

use super::{CaptureState, SelectedFrame, State};

struct StreamListWidget {}

pub fn list_streams(f: &mut Frame, state: &mut State, area: Rect) {
    let border_type = match state.selected_frame {
        SelectedFrame::PacketList => BorderType::Double,
        _ => BorderType::Plain,
    };

    let title = match state.capture_state {
        CaptureState::Active => Span::styled(
            "HTTP streams (capturing)",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        CaptureState::Inactive => Span::raw("HTTP streams"),
    };

    let list = List::new(state.stream_items.clone())
        .block(
            Block::default()
                .title(title)
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_type(border_type),
        )
        .highlight_symbol(">>")
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    f.render_stateful_widget(list, area, &mut state.selected_stream);
}
