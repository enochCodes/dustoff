//! Render smoke tests. These catch the failure mode a TUI is most prone to:
//! a layout that panics or silently clips at an unusual terminal size.

use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::{App, Pane, Screen};
use crate::rules::{Act, Cat, Risk};
use crate::scan::{Item, Mode, Outcome};
use crate::ui;

fn item(cat: Cat, label: &'static str, risk: Risk, bytes: u64, path: &str) -> Item {
    Item {
        cat,
        label,
        detail: "A cache that regenerates on next use.",
        hint: "",
        risk,
        act: Act::Clear,
        path: PathBuf::from(path),
        bytes,
        files: 128,
        selected: false,
        blocked: None,
    }
}

fn demo() -> App {
    let mut app = App::blank(Mode::Dry);
    app.items = vec![
        item(
            Cat::Agents,
            "Claude Code · session transcripts",
            Risk::Careful,
            2_950_000_000,
            "/home/u/.claude/projects",
        ),
        item(
            Cat::Agents,
            "Ollama · local models",
            Risk::Careful,
            18_400_000_000,
            "/home/u/.ollama/models",
        ),
        item(
            Cat::Agents,
            "Claude Code · telemetry cache",
            Risk::Safe,
            41_000_000,
            "/home/u/.claude/statsig",
        ),
        item(
            Cat::Dev,
            "Go · build cache",
            Risk::Safe,
            7_090_000_000,
            "/home/u/Library/Caches/go-build",
        ),
        item(
            Cat::Dev,
            "Eclipse JDT · workspaces",
            Risk::Safe,
            231_000_000,
            "/home/u/Library/Caches/jdtls-abc",
        ),
        item(
            Cat::Dev,
            "Eclipse JDT · workspaces",
            Risk::Safe,
            196_000_000,
            "/home/u/Library/Caches/jdtls-def",
        ),
        item(
            Cat::Dev,
            "Cargo · registry sources",
            Risk::Rebuildable,
            1_480_000_000,
            "/home/u/.cargo/registry/src",
        ),
        item(
            Cat::Browsers,
            "Brave · cache",
            Risk::Safe,
            3_610_000_000,
            "/home/u/Library/Caches/BraveSoftware",
        ),
    ];
    let mut blocked = item(
        Cat::System,
        "Something refused",
        Risk::Safe,
        0,
        "/etc/passwd",
    );
    blocked.blocked = Some("outside $HOME and /Volumes".into());
    app.items.push(blocked);

    let mut advisory = item(
        Cat::Manual,
        "Time Machine local snapshots",
        Risk::Rebuildable,
        0,
        "",
    );
    advisory.act = Act::Advise;
    advisory.hint = "sudo tmutil thinlocalsnapshots / 999999999999 4";
    app.items.push(advisory);

    app.items[3].selected = true;
    app.items[7].selected = true;
    app
}

fn render(app: &mut App, w: u16, h: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw(f, app)).unwrap();
    let buf = term.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn renders_the_browse_screen() {
    let mut app = demo();
    let out = render(&mut app, 104, 30);
    assert!(out.contains("dustoff"));
    assert!(out.contains("AI agents"));
    // Category 0 is selected, so its rows are the ones on screen.
    assert!(out.contains("Ollama"));
    assert!(out.contains("reclaimable"));
    // The repeated-label disambiguator should surface the differing basenames.
    app.cat_idx = 1;
    let out = render(&mut app, 104, 30);
    assert!(
        out.contains("jdtls-abc"),
        "duplicate labels need a path suffix"
    );
    assert!(out.contains("jdtls-def"));
    println!("\n{out}");
}

#[test]
fn renders_every_overlay() {
    for screen in [
        Screen::Confirm,
        Screen::Working,
        Screen::Report,
        Screen::Help,
    ] {
        let mut app = demo();
        app.screen = screen;
        app.outcome = Some(Outcome {
            freed: 10_700_000_000,
            removed: 2,
            failed: vec![(PathBuf::from("/home/u/x"), "permission denied".into())],
        });
        let out = render(&mut app, 104, 30);
        assert!(!out.trim().is_empty());
    }
}

#[test]
fn survives_cramped_and_huge_terminals() {
    // The constraint stack reserves 4 + 5 + 1 rows; a short terminal must not panic.
    for (w, h) in [(20, 8), (40, 10), (80, 24), (300, 90), (60, 11)] {
        let mut app = demo();
        let _ = render(&mut app, w, h);
        app.screen = Screen::Help;
        let _ = render(&mut app, w, h);
        app.screen = Screen::Confirm;
        let _ = render(&mut app, w, h);
    }
}

#[test]
fn selection_helpers_respect_risk_and_blocking() {
    let mut app = demo();
    app.clear_selection();
    app.select_recommended();

    // Careful items are never picked up by "recommended".
    for it in &app.items {
        if it.risk == Risk::Careful {
            assert!(!it.selected, "{} should not be auto-selected", it.label);
        }
    }
    // Blocked and advisory rows are never selectable at all.
    assert!(
        app.items
            .iter()
            .all(|i| !(i.selected && i.blocked.is_some()))
    );
    assert!(app.items.iter().all(|i| !(i.selected && i.advisory())));
    assert!(app.selected_bytes() > 0);

    app.clear_selection();
    assert_eq!(app.selected_count(), 0);
}

#[test]
fn category_toggle_only_touches_the_visible_category() {
    let mut app = demo();
    app.clear_selection();
    app.pane = Pane::Items;
    app.cat_idx = 1; // Build & package caches
    app.toggle_category();

    let dev_on = app
        .items
        .iter()
        .filter(|i| i.cat == Cat::Dev && i.selected)
        .count();
    let other_on = app
        .items
        .iter()
        .filter(|i| i.cat != Cat::Dev && i.selected)
        .count();
    assert!(dev_on > 0);
    assert_eq!(other_on, 0, "toggling one category must not reach another");
}

#[test]
fn filtering_narrows_the_visible_rows() {
    let mut app = demo();
    app.cat_idx = 1;
    let all = app.visible().len();
    app.filter = "jdtls".into();
    let narrowed = app.visible().len();
    assert!(narrowed < all && narrowed == 2, "got {narrowed} of {all}");
}
