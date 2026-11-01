use divoom_ditoo_controller::icons::{self, ResolvedIcon};

#[test]
fn test_builtin_icon_resolution() {
    assert!(icons::get_embedded_icon("mail").is_some());
    assert!(icons::get_embedded_icon("email").is_some());
    assert!(icons::get_embedded_icon("chat").is_some());
    assert!(icons::get_embedded_icon("slack").is_some());
    assert!(icons::get_embedded_icon("calendar").is_some());
    assert!(icons::get_embedded_icon("music").is_some());
    assert!(icons::get_embedded_icon("spotify").is_some());
    assert!(icons::get_embedded_icon("terminal").is_some());
    assert!(icons::get_embedded_icon("git").is_some());
    assert!(icons::get_embedded_icon("bell").is_some());
    assert!(icons::get_embedded_icon("claude").is_some());
    assert!(icons::get_embedded_icon("codex").is_some());
    assert!(icons::get_embedded_icon("antigravity").is_some());
    assert!(icons::get_embedded_icon("done").is_some());
    assert!(icons::get_embedded_icon("error").is_some());
    assert!(icons::get_embedded_icon("alert").is_some());
    assert!(icons::get_embedded_icon("busy").is_some());
    assert!(icons::get_embedded_icon("info").is_some());
    assert!(icons::get_embedded_icon("disk").is_some());
    assert!(icons::get_embedded_icon("storage").is_some());
}

#[test]
fn test_resolve_icon_tiers() {
    // 1. Embedded
    let res = icons::resolve_icon("mail");
    assert!(matches!(res, Some(ResolvedIcon::EmbeddedImage(_))));

    let res_claude = icons::resolve_icon("claude");
    assert!(matches!(res_claude, Some(ResolvedIcon::EmbeddedImage(_))));

    // 2. Procedural badge for arbitrary short names
    let res_custom = icons::resolve_icon("zoom");
    assert!(matches!(res_custom, Some(ResolvedIcon::Procedural(_))));
    if let Some(ResolvedIcon::Procedural(path)) = res_custom {
        assert!(path.exists());
        let _ = std::fs::remove_file(path);
    }
}
