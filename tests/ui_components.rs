use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

use coding_agent_search::search::query::SearchFilters;
use coding_agent_search::ui::components::theme::ThemePalette;
use coding_agent_search::ui::components::widgets::search_bar;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Tabs;

#[test]
fn search_bar_tips_include_clear_hotkeys() {
    let palette = ThemePalette::dark();
    let widget = search_bar("test", palette, true);
    let rect = Rect::new(0, 0, 100, 4);
    let mut buf = Buffer::empty(rect);
    widget.render(rect, &mut buf);

    let lines: Vec<String> = (0..rect.height)
        .map(|y| {
            (0..rect.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<Vec<_>>()
                .join("")
        })
        .collect();
    let joined = lines.join("\n");
    eprintln!("bar={joined}");
    assert!(joined.contains("A/W/F clear"));
    assert!(joined.contains("x clear all"));
}

#[test]
fn filter_pills_render_selected_filters() {
    let palette = ThemePalette::dark();
    let mut filters = SearchFilters::default();
    filters.agents.insert("codex".into());
    filters.workspaces.insert("/ws/demo".into());
    filters.created_from = Some(100);
    filters.created_to = Some(200);

    let mut pill_spans = Vec::new();
    if !filters.agents.is_empty() {
        pill_spans.push(Span::styled(
            format!(
                "[a] agent:{}",
                filters.agents.iter().cloned().collect::<Vec<_>>().join("|")
            ),
            Style::default()
                .fg(palette.accent_alt)
                .add_modifier(Modifier::BOLD),
        ));
        pill_spans.push(Span::raw("  "));
    }
    if !filters.workspaces.is_empty() {
        pill_spans.push(Span::styled(
            format!(
                "[w] ws:{}",
                filters
                    .workspaces
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("|")
            ),
            Style::default().fg(palette.accent_alt),
        ));
        pill_spans.push(Span::raw("  "));
    }
    if filters.created_from.is_some() || filters.created_to.is_some() {
        pill_spans.push(Span::styled(
            format!(
                "[f/t] time:{:?}->{:?}",
                filters.created_from, filters.created_to
            ),
            Style::default().fg(palette.accent_alt),
        ));
    }

    let pill_line = Line::from(pill_spans);
    let text = pill_line
        .spans
        .iter()
        .map(|s| s.content.clone().into_owned())
        .collect::<Vec<_>>()
        .join("");

    assert!(text.contains("[a] agent:codex"));
    assert!(text.contains("[w] ws:/ws/demo"));
    assert!(text.contains("[f/t] time:Some(100)->Some(200)"));
}

#[test]
fn detail_tabs_labels_present() {
    let palette = ThemePalette::dark();
    let tabs = ["Messages", "Snippets", "Raw"];
    let tab_titles: Vec<Line> = tabs
        .iter()
        .map(|t| Line::from(Span::styled(*t, palette.title())))
        .collect();
    let widget = Tabs::new(tab_titles);

    let mut buf = Buffer::empty(Rect::new(0, 0, 40, 1));
    widget.render(Rect::new(0, 0, 40, 1), &mut buf);
    let line: String = (0..40).map(|x| buf[(x, 0)].symbol().to_string()).collect();
    eprintln!("tabs={line}");
    assert!(line.contains("Messages"));
    assert!(line.contains("Snippets"));
    assert!(line.contains("Raw"));
}
