//! The expandable prompt box: wraps, grows to 5 lines, then scrolls internally.

use crate::app::{App, JobKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::theme::Theme;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let mic_width = if area.width >= 35 { 13 } else { 0 };
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
        
        let mut mic_lines = Vec::new();
        let inner_height = mic_area.height.saturating_sub(2) as usize;

        if is_active {
            // High-energy pulsing gradient colors for border and active equalizer
            let border_colors = [
                Color::Rgb(255, 60, 60),
                Color::Rgb(255, 120, 40),
                Color::Rgb(255, 40, 140),
                Color::Rgb(220, 50, 255),
                Color::Rgb(255, 50, 80),
            ];
            let border_color = border_colors[(tick / 2) % border_colors.len()];

            // Equalizer frames with dynamic bouncing bars
            let eq_frames: [&[&str]; 6] = [
                &[" ", "▃", "▆", "█", "▄", "▂"],
                &["▃", "▆", "█", "▅", "▇", "▄"],
                &["▆", "█", "▄", "▂", "▅", "▇"],
                &["█", "▅", "▂", "▄", "▇", "█"],
                &["▅", "▂", "▅", "▇", "█", "▅"],
                &["▂", "▄", "▇", "█", "▃", " "],
            ];
            let current_bars = eq_frames[tick % eq_frames.len()];
            
            // Bar gradient palette
            let bar_palette = [
                Color::Rgb(255, 80, 80),
                Color::Rgb(255, 140, 50),
                Color::Rgb(255, 220, 60),
                Color::Rgb(255, 100, 150),
                Color::Rgb(200, 80, 255),
                Color::Rgb(255, 50, 100),
            ];

            let mut eq_spans = vec![Span::styled("🎙 ", Color::Rgb(255, 80, 80))];
            for (i, &bar) in current_bars.iter().enumerate() {
                let color_idx = (i + tick) % bar_palette.len();
                eq_spans.push(Span::styled(bar, bar_palette[color_idx]));
            }

            if inner_height > 1 {
                let top_padding = (inner_height.saturating_sub(2)) / 2;
                for _ in 0..top_padding {
                    mic_lines.push(Line::from(""));
                }
                mic_lines.push(Line::from(vec![
                    Span::styled("● REC", Color::Rgb(255, 50, 50)),
                ]).alignment(ratatui::layout::Alignment::Center));
                mic_lines.push(Line::from(eq_spans).alignment(ratatui::layout::Alignment::Center));
            } else {
                mic_lines.push(Line::from(eq_spans).alignment(ratatui::layout::Alignment::Center));
            }

            let mic_block = Block::default()
                .borders(Borders::ALL)
                .title(" mic ")
                .title_style(ratatui::style::Style::default().fg(border_color))
                .border_style(ratatui::style::Style::default().fg(border_color));
                
            frame.render_widget(Paragraph::new(mic_lines).block(mic_block), mic_area);
        } else {
            // Idle state: subtle, aesthetic idle indicator
            let idle_spans = vec![
                Span::styled("🎤 ", theme.muted()),
                Span::styled("···", theme.muted()),
            ];

            if inner_height > 1 {
                let top_padding = (inner_height.saturating_sub(1)) / 2;
                for _ in 0..top_padding {
                    mic_lines.push(Line::from(""));
                }
            }
            mic_lines.push(Line::from(idle_spans).alignment(ratatui::layout::Alignment::Center));

            let mic_block = Block::default()
                .borders(Borders::ALL)
                .title(" mic ")
                .title_style(theme.muted())
                .border_style(theme.muted());
                
            frame.render_widget(Paragraph::new(mic_lines).block(mic_block), mic_area);
        }
    }
}
