use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Gauge, Paragraph, Wrap};

use crate::app::{App, Pane, Screen};
use crate::rules::{Cat, Risk};
use crate::scan::Mode;
use crate::theme as t;
use crate::util::{bytes, tilde};

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(t::BG)), area);

    let [head, body, detail, foot] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(6),
        Constraint::Length(5),
        Constraint::Length(1),
    ])
    .areas(area);

    header(f, app, head);

    let [side, list] =
        Layout::horizontal([Constraint::Length(30), Constraint::Min(30)]).areas(body);
    sidebar(f, app, side);
    items(f, app, list);
    detail_pane(f, app, detail);
    footer(f, app, foot);

    match app.screen {
        Screen::Confirm => confirm(f, app, area),
        Screen::Working => working(f, area),
        Screen::Report => report(f, app, area),
        Screen::Help => help(f, area),
        Screen::Browse => {}
    }
}

fn panel(title: &str, focused: bool) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { t::ACCENT } else { t::FAINT }))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                title.to_string(),
                Style::default()
                    .fg(if focused { t::ACCENT } else { t::DIM })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]))
        .style(Style::default().bg(t::PANEL))
}

fn header(f: &mut Frame, app: &App, area: Rect) {
    let d = app.disk;
    let title = Line::from(vec![
        Span::styled(" dustoff ", Style::default().fg(t::BG).bg(t::ACCENT).bold()),
        Span::raw("  "),
        Span::styled(
            format!("{} reclaimable", bytes(app.total_bytes())),
            Style::default().fg(t::FG).bold(),
        ),
        Span::styled(
            format!("  ·  {} items", app.items.len()),
            Style::default().fg(t::DIM),
        ),
        Span::styled(
            if app.selected_count() > 0 {
                format!(
                    "  ·  {} selected in {} items",
                    bytes(app.selected_bytes()),
                    app.selected_count()
                )
            } else {
                String::new()
            },
            Style::default().fg(t::ACCENT),
        ),
        Span::styled(
            if app.blocked_count() > 0 {
                format!("  ·  {} refused", app.blocked_count())
            } else {
                String::new()
            },
            Style::default().fg(t::BLOCKED),
        ),
    ]);

    let status = if app.scanning {
        Line::from(vec![
            Span::styled(
                format!(" {} ", t::SPINNER[app.tick % t::SPINNER.len()]),
                Style::default().fg(t::ACCENT),
            ),
            Span::styled("scanning  ", Style::default().fg(t::FG)),
            Span::styled(app.probing.clone(), Style::default().fg(t::DIM).italic()),
        ])
    } else {
        Line::from(vec![
            Span::styled(" disk  ", Style::default().fg(t::DIM)),
            Span::styled(bytes(d.used), Style::default().fg(t::FG)),
            Span::styled(" used of ", Style::default().fg(t::DIM)),
            Span::styled(bytes(d.total), Style::default().fg(t::FG)),
            Span::styled("  ·  ", Style::default().fg(t::FAINT)),
            Span::styled(bytes(d.free), Style::default().fg(t::SAFE)),
            Span::styled(" free", Style::default().fg(t::DIM)),
            Span::styled(
                format!("   mode: {}", app.mode_label()),
                Style::default().fg(match app.mode {
                    Mode::Dry => t::DIM,
                    Mode::Trash => t::REBUILD,
                    Mode::Permanent => t::CAREFUL,
                }),
            ),
        ])
    };

    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(t::FAINT));
    f.render_widget(block, area);

    let inner = area.inner(Margin::new(1, 0));
    let [l1, l2, l3] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    f.render_widget(Paragraph::new(title), l1);
    f.render_widget(Paragraph::new(status), l2);

    let g = Gauge::default()
        .ratio(d.ratio())
        .gauge_style(Style::default().fg(if d.ratio() > 0.9 {
            t::CAREFUL
        } else {
            t::ACCENT_DK
        }))
        .label(Span::styled(
            format!("{:.0}% full", d.ratio() * 100.0),
            Style::default().fg(t::FG),
        ));
    f.render_widget(g, l3.inner(Margin::new(1, 0)));
}

fn sidebar(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.pane == Pane::Cats;
    let block = panel("categories", focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cats = app.cats();
    // marker(2) + glyph(2) + size(8) leaves the rest for the name.
    let namew = (inner.width as usize).saturating_sub(12).max(6);
    let mut lines: Vec<Line> = Vec::new();
    for (i, c) in cats.iter().enumerate() {
        let on = i == app.cat_idx;
        let total = app.cat_bytes(*c);
        let sel = app.cat_selected_bytes(*c);
        let color = t::cat_color(*c);

        // An advisory category has no size, and a zero is a worse answer than a dash.
        let size_text = if *c == Cat::Manual || total == 0 {
            "—".to_string()
        } else {
            bytes(total)
        };

        lines.push(Line::from(vec![
            Span::styled(if on { " ▌" } else { "  " }, Style::default().fg(t::ACCENT)),
            Span::styled(format!("{} ", c.glyph()), Style::default().fg(color)),
            Span::styled(
                format!("{:<w$}", trunc(c.title(), namew), w = namew),
                if on {
                    Style::default().fg(t::FG).bold()
                } else {
                    Style::default().fg(t::FG)
                },
            ),
            Span::styled(
                format!("{size_text:>8}"),
                Style::default().fg(if sel > 0 { t::ACCENT } else { t::DIM }),
            ),
        ]));
    }
    if cats.is_empty() {
        lines.push(Line::from(Span::styled(
            if app.scanning {
                "  scanning…"
            } else {
                "  nothing found"
            },
            Style::default().fg(t::DIM),
        )));
    }

    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(t::PANEL)),
        inner,
    );
}

fn items(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.pane == Pane::Items;
    let title = match app.cat() {
        Some(c) => format!("{}  ·  {}", c.title(), bytes(app.cat_bytes(c))),
        None => "items".into(),
    };
    let block = panel(&title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let height = inner.height as usize;
    app.clamp_scroll(height);

    let vis = app.visible();
    let peak = app.cat().map(|c| app.cat_peak(c)).unwrap_or(1);
    let barw = 10usize;

    // A rule that yields several paths (a glob, or a per-child folder) repeats
    // its label. Disambiguate those rows with the final path component.
    let mut seen: Vec<(&str, usize)> = Vec::new();
    for &n in &vis {
        let l = app.items[n].label;
        match seen.iter_mut().find(|(s, _)| *s == l) {
            Some((_, c)) => *c += 1,
            None => seen.push((l, 1)),
        }
    }
    let repeated = |l: &str| seen.iter().any(|(s, c)| *s == l && *c > 1);

    let mut lines: Vec<Line> = Vec::new();
    for (vi, &n) in vis.iter().enumerate().skip(app.top).take(height) {
        let it = &app.items[n];
        let here = vi == app.row && focused;
        let rowbg = if vi == app.row { t::SEL } else { t::PANEL };

        let check = if it.advisory() {
            Span::styled("  ⚙ ", Style::default().fg(t::DIM))
        } else if it.blocked.is_some() {
            Span::styled("  ⊘ ", Style::default().fg(t::BLOCKED))
        } else if it.selected {
            Span::styled("  ◉ ", Style::default().fg(t::ACCENT))
        } else {
            Span::styled("  ○ ", Style::default().fg(t::FAINT))
        };

        let frac = if peak > 0 {
            it.bytes as f64 / peak as f64
        } else {
            0.0
        };
        let bar_span = if it.advisory() {
            Span::styled(" ".repeat(barw), Style::default())
        } else {
            Span::styled(
                t::bar(frac, barw),
                Style::default().fg(if it.selected {
                    t::ACCENT
                } else {
                    t::risk_color(it.risk)
                }),
            )
        };

        let size = if it.advisory() {
            Span::styled("       —", Style::default().fg(t::DIM))
        } else {
            Span::styled(
                format!("{:>8}", bytes(it.bytes)),
                Style::default().fg(t::FG).bold(),
            )
        };

        let risk = Span::styled(
            format!(" {:<8}", it.risk.label()),
            Style::default().fg(t::risk_color(it.risk)),
        );

        let room = inner.width.saturating_sub(36) as usize;
        let suffix = if repeated(it.label) {
            it.path
                .file_name()
                .map(|s| format!("  {}", s.to_string_lossy()))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let label_room = room.saturating_sub(suffix.chars().count());

        lines.push(
            Line::from(vec![
                check,
                bar_span,
                Span::raw(" "),
                size,
                risk,
                Span::styled(
                    trunc(it.label, label_room),
                    if here {
                        Style::default().fg(t::FG).bold()
                    } else {
                        Style::default().fg(t::FG)
                    },
                ),
                Span::styled(
                    trunc(&suffix, room.saturating_sub(1)),
                    Style::default().fg(t::DIM),
                ),
            ])
            .style(Style::default().bg(rowbg)),
        );
    }

    if vis.is_empty() {
        lines.push(Line::from(Span::styled(
            if app.scanning {
                "  scanning…"
            } else {
                "  nothing here"
            },
            Style::default().fg(t::DIM),
        )));
    }

    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(t::PANEL)),
        inner,
    );

    // Scroll position, drawn as a thin marker rather than a full scrollbar.
    if vis.len() > height && height > 0 {
        let frac = app.row as f64 / (vis.len().saturating_sub(1)).max(1) as f64;
        let y = inner.y + (frac * (height.saturating_sub(1)) as f64).round() as u16;
        let x = inner.x + inner.width.saturating_sub(1);
        if y < inner.y + inner.height {
            f.render_widget(
                Paragraph::new(Span::styled("▐", Style::default().fg(t::ACCENT_DK))),
                Rect::new(x, y, 1, 1),
            );
        }
    }
}

fn detail_pane(f: &mut Frame, app: &App, area: Rect) {
    let block = panel("detail", false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    if let Some(msg) = app.toast_text() {
        lines.push(Line::from(Span::styled(
            format!("  {msg}"),
            Style::default().fg(t::ACCENT).bold(),
        )));
    }

    if let Some(it) = app.current() {
        if lines.is_empty() {
            let loc = if it.advisory() {
                format!("  $ {}", it.hint)
            } else {
                format!(
                    "  {}   {} files",
                    tilde(&it.path),
                    if it.files > 0 {
                        it.files.to_string()
                    } else {
                        "—".into()
                    }
                )
            };
            lines.push(Line::from(Span::styled(
                loc,
                Style::default().fg(if it.advisory() { t::REBUILD } else { t::ACCENT }),
            )));
        }
        lines.push(Line::from(Span::styled(
            format!("  {}", it.detail),
            Style::default().fg(t::FG),
        )));
        if let Some(r) = &it.blocked {
            lines.push(Line::from(Span::styled(
                format!("  refused by safety check — {r}"),
                Style::default().fg(t::BLOCKED),
            )));
        }
    } else if let Some(c) = app.cat() {
        lines.push(Line::from(Span::styled(
            format!("  {}", c.blurb()),
            Style::default().fg(t::DIM),
        )));
    }

    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(t::PANEL)),
        inner,
    );
}

fn key(k: &str, what: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(k.to_string(), Style::default().fg(t::ACCENT).bold()),
        Span::styled(format!(" {what}  "), Style::default().fg(t::DIM)),
    ]
}

fn footer(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![Span::raw(" ")];
    if app.filtering {
        spans.push(Span::styled("filter: ", Style::default().fg(t::DIM)));
        spans.push(Span::styled(
            format!("{}▏", app.filter),
            Style::default().fg(t::ACCENT).bold(),
        ));
        spans.push(Span::styled(
            "   enter accept · esc clear",
            Style::default().fg(t::FAINT),
        ));
    } else {
        for (k, w) in [
            ("space", "select"),
            ("a", "category"),
            ("A", "recommended"),
            ("d", "clean"),
            ("t", "mode"),
            ("/", "find"),
            ("r", "rescan"),
            ("?", "help"),
            ("q", "quit"),
        ] {
            spans.extend(key(k, w));
        }
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(t::BG)),
        area,
    );
}

fn popup(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(4));
    let h = h.min(area.height.saturating_sub(4));
    Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    )
}

fn modal<'a>(title: &'a str, color: ratatui::style::Color) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(title, Style::default().fg(color).bold()),
            Span::raw(" "),
        ]))
        .title_alignment(Alignment::Center)
        .style(Style::default().bg(t::PANEL))
}

fn confirm(f: &mut Frame, app: &App, area: Rect) {
    let r = popup(area, 66, 15);
    f.render_widget(Clear, r);

    let careful: Vec<&str> = app
        .items
        .iter()
        .filter(|i| i.selected && i.risk == Risk::Careful)
        .map(|i| i.label)
        .collect();

    let (verb, color) = match app.mode {
        Mode::Dry => ("Simulate", t::DIM),
        Mode::Trash => ("Move to Trash", t::REBUILD),
        Mode::Permanent => ("Delete permanently", t::CAREFUL),
    };

    let block = modal(verb, color);
    let inner = block.inner(r);
    f.render_widget(block, r);

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("   ", Style::default()),
            Span::styled(
                bytes(app.selected_bytes()),
                Style::default().fg(t::FG).bold(),
            ),
            Span::styled(
                format!("  across {} items", app.selected_count()),
                Style::default().fg(t::DIM),
            ),
        ]),
        Line::from(""),
    ];

    match app.mode {
        Mode::Dry => lines.push(Line::from(Span::styled(
            "   Dry run: nothing is touched. Press t to change mode.",
            Style::default().fg(t::DIM),
        ))),
        Mode::Trash => lines.push(Line::from(Span::styled(
            "   Items go to the Trash and can be put back.",
            Style::default().fg(t::SAFE),
        ))),
        Mode::Permanent => lines.push(Line::from(Span::styled(
            "   Unlinked immediately. There is no undo.",
            Style::default().fg(t::CAREFUL),
        ))),
    }

    if !careful.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("   {} item(s) marked careful:", careful.len()),
            Style::default().fg(t::CAREFUL).bold(),
        )));
        for c in careful.iter().take(3) {
            lines.push(Line::from(Span::styled(
                format!("     · {c}"),
                Style::default().fg(t::CAREFUL),
            )));
        }
        if careful.len() > 3 {
            lines.push(Line::from(Span::styled(
                format!("     · and {} more", careful.len() - 3),
                Style::default().fg(t::CAREFUL),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("   "),
        Span::styled("y", Style::default().fg(t::ACCENT).bold()),
        Span::styled(" proceed    ", Style::default().fg(t::DIM)),
        Span::styled("n", Style::default().fg(t::ACCENT).bold()),
        Span::styled(" cancel", Style::default().fg(t::DIM)),
    ]));

    f.render_widget(Paragraph::new(lines), inner);
}

fn working(f: &mut Frame, area: Rect) {
    let r = popup(area, 40, 5);
    f.render_widget(Clear, r);
    let block = modal("working", t::ACCENT);
    let inner = block.inner(r);
    f.render_widget(block, r);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "removing…",
            Style::default().fg(t::FG),
        )))
        .alignment(Alignment::Center),
        inner,
    );
}

fn report(f: &mut Frame, app: &App, area: Rect) {
    let out = app.outcome.as_ref();
    let r = popup(area, 70, 16);
    f.render_widget(Clear, r);
    let block = modal(
        if app.mode == Mode::Dry {
            "dry run result"
        } else {
            "done"
        },
        t::SAFE,
    );
    let inner = block.inner(r);
    f.render_widget(block, r);

    let mut lines = vec![Line::from("")];
    if let Some(o) = out {
        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(bytes(o.freed), Style::default().fg(t::SAFE).bold()),
            Span::styled(
                if app.mode == Mode::Dry {
                    format!("  would be reclaimed from {} items", o.removed)
                } else {
                    format!("  reclaimed from {} items", o.removed)
                },
                Style::default().fg(t::FG),
            ),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("   now free: ", Style::default().fg(t::DIM)),
            Span::styled(bytes(app.disk.free), Style::default().fg(t::FG).bold()),
        ]));

        if !o.failed.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("   {} could not be removed:", o.failed.len()),
                Style::default().fg(t::CAREFUL).bold(),
            )));
            for (p, e) in o.failed.iter().take(4) {
                lines.push(Line::from(Span::styled(
                    format!("     {} — {}", trunc(&tilde(p), 40), trunc(e, 20)),
                    Style::default().fg(t::DIM),
                )));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "   macOS reports purgeable space lazily; About This Mac may lag.",
            Style::default().fg(t::DIM).italic(),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("   "),
        Span::styled("r", Style::default().fg(t::ACCENT).bold()),
        Span::styled(" rescan    ", Style::default().fg(t::DIM)),
        Span::styled("enter", Style::default().fg(t::ACCENT).bold()),
        Span::styled(" back    ", Style::default().fg(t::DIM)),
        Span::styled("q", Style::default().fg(t::ACCENT).bold()),
        Span::styled(" quit", Style::default().fg(t::DIM)),
    ]));

    f.render_widget(Paragraph::new(lines), inner);
}

fn help(f: &mut Frame, area: Rect) {
    let r = popup(area, 74, 24);
    f.render_widget(Clear, r);
    let block = modal("keys & conventions", t::ACCENT);
    let inner = block.inner(r);
    f.render_widget(block, r);

    let row = |k: &'static str, d: &'static str| {
        Line::from(vec![
            Span::styled(format!("   {k:<14}"), Style::default().fg(t::ACCENT).bold()),
            Span::styled(d, Style::default().fg(t::FG)),
        ])
    };

    let lines = vec![
        Line::from(""),
        row("↑ ↓ / j k", "move"),
        row("← → / h l", "switch pane"),
        row("space", "select or deselect the highlighted item"),
        row("a", "select or deselect everything in this category"),
        row("A", "select all safe + rebuildable items, everywhere"),
        row("n", "clear the whole selection"),
        row("d", "clean the selection (asks first)"),
        row("t", "cycle mode: dry run → trash → permanent"),
        row("c", "copy a Needs-Terminal command to the clipboard"),
        row("/", "filter by name or path"),
        row("r", "rescan from scratch"),
        row("g / G", "jump to top / bottom"),
        row("q", "quit"),
        Line::from(""),
        Line::from(vec![
            Span::styled("   safe     ", Style::default().fg(t::SAFE).bold()),
            Span::styled(
                "pure cache; costs a cold start and nothing else",
                Style::default().fg(t::DIM),
            ),
        ]),
        Line::from(vec![
            Span::styled("   rebuild  ", Style::default().fg(t::REBUILD).bold()),
            Span::styled(
                "re-downloads or recompiles; costs time, never data",
                Style::default().fg(t::DIM),
            ),
        ]),
        Line::from(vec![
            Span::styled("   careful  ", Style::default().fg(t::CAREFUL).bold()),
            Span::styled(
                "history, archives or models you may actually want",
                Style::default().fg(t::DIM),
            ),
        ]),
        Line::from(vec![
            Span::styled("   ⊘        ", Style::default().fg(t::BLOCKED).bold()),
            Span::styled(
                "refused by the safety layer; shown, never deletable",
                Style::default().fg(t::DIM),
            ),
        ]),
    ];

    f.render_widget(Paragraph::new(lines), inner);
}

fn trunc(s: &str, w: usize) -> String {
    if w == 0 {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= w {
        s.to_string()
    } else {
        let mut out: String = chars[..w.saturating_sub(1)].iter().collect();
        out.push('…');
        out
    }
}
