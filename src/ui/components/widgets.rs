use ratatui::layout::Alignment;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::ui::components::theme::ThemePalette;
use crate::ui::data::InputMode;
use ratatui::widgets::Wrap;

pub fn search_bar(
    query: &str,
    palette: ThemePalette,
    input_mode: InputMode,
    mode_label: &str,
    chips: Vec<Span<'static>>,
) -> Paragraph<'static> {
    let focused = matches!(input_mode, InputMode::Query);
    let title = Span::styled(format!("Search · {mode_label}"), palette.title());
    let style = if focused {
        Style::default().fg(palette.accent)
    } else {
        Style::default().fg(palette.hint)
    };

    let border_style = match input_mode {
        InputMode::Query => Style::default().fg(palette.accent_alt),
        _ => Style::default().fg(palette.accent).bg(palette.surface),
    };

    let mut first_line = chips;
    if !first_line.is_empty() {
        first_line.push(Span::raw(" "));
    }
    first_line.push(Span::styled(format!("/ {}", query), style));

    let body = vec![
        Line::from(first_line),
        Line::from(vec![
            Span::styled("Tips: ", palette.title()),
            Span::raw(
                "F3 agent • F4 workspace • F5/F6 time • F7 context • F11 clear • F9 mode • F2 theme • F8/Enter open • Ctrl-R history",
            ),
        ]),
    ];

    Paragraph::new(body)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .style(Style::default())
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true })
}
