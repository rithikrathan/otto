//! The expandable prompt box: wraps, grows to 5 lines, then scrolls internally.

use crate::app::{App, JobKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::theme::Theme;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let mic_width = if area.height >= 5 { 11 } else { 0 };
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(mic_width)])
        .split(area);
        
    let prompt_area = chunks[0];
    let prompt = &mut app.prompt;

    let width = prompt_area.width.saturating_sub(4) as usize; // minus borders and horizontal padding
    if width == 0 { return; }
    prompt.width = width;

    let mut lines = Vec::new();
    let mut row: usize = 0;
    let mut col: usize = 0;
    let mut current_line = String::new();
    let mut cursor_row: usize = 0;
    let mut cursor_col: usize = 0;

    let text_len = prompt.text.len();
    for (i, ch) in prompt.text.char_indices() {
        if i == prompt.cursor {
            cursor_row = row;
            cursor_col = col;
        }
        if ch == '\n' {
            lines.push(Line::from(current_line.clone()));
            current_line.clear();
            row += 1;
            col = 0;
        } else if col >= width {
            lines.push(Line::from(current_line.clone()));
            current_line.clear();
            current_line.push(ch);
            row += 1;
            col = 1;
        } else {
            current_line.push(ch);
            col += 1;
        }
    }
    if prompt.cursor >= text_len {
        cursor_row = row;
        cursor_col = col;
    }
    
    let mut last_spans = vec![ratatui::text::Span::raw(current_line)];
    
    // Ghost text for slash command autocomplete
    if prompt.cursor == text_len {
        if let Some(comp) = crate::cmd::autocomplete(&prompt.text) {
            last_spans.push(ratatui::text::Span::styled(comp, theme.muted()));
        }
    }
    
    lines.push(Line::from(last_spans));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" input ")
        .title_style(theme.muted())
        .border_style(theme.muted())
        .padding(ratatui::widgets::Padding::horizontal(1));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((prompt.scroll as u16, 0));

    frame.render_widget(paragraph, prompt_area);

    // Draw the cursor manually using a reversed block to prevent hardware cursor flicker.
    let x = prompt_area.x + 2 + cursor_col as u16; // 1 for border, 1 for padding
    let y = prompt_area.y + 1 + cursor_row.saturating_sub(prompt.scroll) as u16;
    if x < prompt_area.x + prompt_area.width && y < prompt_area.y + prompt_area.height {
        if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
            let symbol = cell.symbol();
            if symbol == " " || symbol.is_empty() {
                cell.set_symbol("█");
                // match the muted theme color or just default
                cell.set_fg(theme.muted().fg.unwrap_or(ratatui::style::Color::DarkGray));
            } else {
                cell.set_style(cell.style().add_modifier(ratatui::style::Modifier::REVERSED));
            }
        }
    }
    
    if mic_width > 0 {
        let mic_area = chunks[1];
        let is_active = app.busy.contains(&JobKind::Stt);
        let tick = app.tick as usize;
        
        let (fg, border_color) = if is_active {
            let active_colors = [
                Color::Rgb(255, 50, 50),
                Color::Rgb(255, 100, 50),
                Color::Rgb(255, 150, 50),
                Color::Rgb(255, 50, 100),
                Color::Rgb(255, 0, 50),
            ];
            (active_colors[(tick / 2) % active_colors.len()], Color::Rgb(255, 50, 50))
        } else {
            let inactive_colors = [
                Color::DarkGray,
                Color::Rgb(100, 100, 100),
                Color::Gray,
                Color::Rgb(100, 100, 100),
            ];
            (inactive_colors[(tick / 5) % inactive_colors.len()], Color::DarkGray)
        };
        
        // Microphone symbol centered vertically
        let symbol = if is_active { " 🎙️ " } else { " 🎤 " };
        let mut mic_lines = vec![Line::from(""); (mic_area.height as usize).saturating_sub(3) / 2];
        mic_lines.push(Line::from(vec![Span::styled(symbol, ratatui::style::Style::default().fg(fg))]).alignment(ratatui::layout::Alignment::Center));
        
        let mic_block = Block::default()
            .borders(Borders::ALL)
            .border_style(ratatui::style::Style::default().fg(border_color));
            
        frame.render_widget(Paragraph::new(mic_lines).block(mic_block), mic_area);
    }
}
