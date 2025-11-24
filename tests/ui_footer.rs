use coding_agent_search::ui::tui::footer_legend;

#[test]
fn footer_legend_toggles_help() {
    let hidden = footer_legend(false);
    assert!(hidden.contains("F1 help"));
    assert!(hidden.contains("Esc/F10 quit"));

    let shown = footer_legend(true);
    assert!(shown.contains("Esc/F10 quit"));
    assert!(shown.contains("F11 clear"));
}
