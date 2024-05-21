use ratatui::{layout::Rect, Frame};

/// Center a Rect on the current Frame.
pub fn center_rect(f: &Frame, width: u16, height: u16) -> Rect {
    let vertical_margin = (f.size().height - height) / 2;
    let horizontal_margin = (f.size().width - width) / 2;

    Rect::new(horizontal_margin, vertical_margin, width, height)
}
