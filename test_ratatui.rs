use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
fn test(buf: &mut Buffer) {
    let cell = buf.get_mut(0, 0);
    cell.set_symbol("█");
    cell.set_fg(ratatui::style::Color::Red);
}
