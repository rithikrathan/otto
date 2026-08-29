use ratatui::text::Text;
fn main() {
    let doc = String::from("Hello");
    let text: Text = tui_markdown::from_str(&doc);
}
