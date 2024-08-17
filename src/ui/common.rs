use ratatui::{layout::Rect, Frame};

/// Center a Rect on the current Frame.
pub fn center_rect(f: &Frame, width: u16, height: u16) -> Rect {
    let vertical_margin = (f.area().height - height) / 2;
    let horizontal_margin = (f.area().width - width) / 2;

    Rect::new(horizontal_margin, vertical_margin, width, height)
}
