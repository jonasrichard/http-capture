use ratatui::{
    layout::Rect,
    text::Text,
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use super::{SelectedFrame, State};

pub fn request_response(f: &mut Frame, state: &mut State, area: Rect) {
    let mut text = Text::from("");

    if let Some(selected) = state.selected_stream.selected() {
        if let Some(s) = &state.streams.get(selected) {
            let pr = &s.parsed_request;

            text.extend(Text::raw(format!("{} {}\n", pr.method, pr.path)));

            for header in &pr.headers {
                text.extend(Text::raw(format!("{}: {}\n", header.0, header.1)));
            }

            text.extend(Text::raw("\n"));

            if let Some(ref body) = pr.body {
                text.extend(Text::raw(body));
            }

            text.extend(Text::raw("\n"));

            let resp = &s.parsed_response;

            text.extend(Text::raw(format!("{} {}", resp.code, resp.version)));

            for header in &resp.headers {
                text.extend(Text::raw(format!("{}: {}\n", header.0, header.1)));
            }

            text.extend(Text::raw("\n"));

            if let Some(ref body) = resp.body {
                text.extend(Text::raw(body));
            }
        }
    }

    let border_type = match state.selected_frame {
        SelectedFrame::PacketDetails => BorderType::Double,
        _ => BorderType::Plain,
    };

    let content = Paragraph::new(text)
        .block(
            Block::default()
                .title("list")
                .borders(Borders::ALL)
                .border_type(border_type),
        )
        .scroll(state.details_scroll);

    f.render_widget(content, area);
}
