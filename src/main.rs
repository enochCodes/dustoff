//! dustoff — a deep storage cleaner for macOS with a terminal interface.
//!
//! Default behaviour is deliberately timid: it scans, shows, and selects
//! nothing. Every destructive path passes through `danger::vet`.

mod app;
mod danger;
#[cfg(test)]
mod render_tests;
mod rules;
mod scan;
mod theme;
mod ui;
mod util;

use std::time::{Duration, Instant};

use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::{App, Pane, Screen};
use crate::rules::{Cat, Risk};
use crate::scan::Mode;
use crate::util::bytes;

#[derive(Parser, Debug)]
#[command(
    name = "dustoff",
    version,
    about = "Deep storage cleaner with a terminal interface",
    long_about = "Finds reclaimable caches across AI coding agents, build tools, \
                  browsers, Xcode, and system junk.\n\n\
                  With no flags it opens an interactive interface and deletes nothing \
                  until you say so."
)]
struct Cli {
    /// Print a ranked report and exit. No interface, no deletion.
    #[arg(long)]
    scan: bool,

    /// Move items to the Trash instead of simulating (interactive default is dry run).
    #[arg(long)]
    trash: bool,

    /// Delete permanently. Implies you know there is no undo.
    #[arg(long, conflicts_with = "trash")]
    permanent: bool,

    /// Clean the recommended set (every safe and rebuildable item) without the
    /// interface. Combine with --trash or --permanent; on its own it only reports.
    #[arg(long)]
    clean: bool,

    /// With --clean, skip the confirmation prompt. For cron and scripts.
    #[arg(long, short = 'y')]
    yes: bool,

    /// With --scan, also list every individual path rather than category totals.
    #[arg(long)]
    verbose: bool,
}

/// Collect the whole catalog without a terminal. Shared by every headless path.
fn collect() -> Vec<scan::Item> {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc::channel;

    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let handle = std::thread::spawn(move || scan::run(rules::catalog(), tx, cancel));

    let mut items = Vec::new();
    while let Ok(msg) = rx.recv() {
        match msg {
            scan::Msg::Found(it) => items.push(*it),
            scan::Msg::Done => break,
            scan::Msg::Probing(_) => {}
        }
    }
    let _ = handle.join();
    items.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    items
}

/// Rust ignores SIGPIPE at startup, which turns `dustoff --scan | head` into a
/// panic instead of a clean exit. Restore the default disposition.
#[cfg(unix)]
fn restore_sigpipe() {
    // SAFETY: setting a signal disposition to SIG_DFL before any threads are
    // spawned is the documented, async-signal-safe way to undo Rust's default.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_sigpipe() {}

fn main() -> anyhow::Result<()> {
    restore_sigpipe();
    let cli = Cli::parse();

    let mode = if cli.permanent {
        Mode::Permanent
    } else if cli.trash {
        Mode::Trash
    } else {
        Mode::Dry
    };

    if cli.scan {
        return headless(cli.verbose);
    }
    if cli.clean {
        return clean_headless(mode, cli.yes);
    }

    // A panic inside the alternate screen would otherwise leave the terminal in
    // raw mode with no echo. Restore first, then let the default hook report.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        default_hook(info);
    }));

    let mut terminal = ratatui::init();
    let res = run(&mut terminal, mode);
    ratatui::restore();
    res
}

/// Clean the recommended set with no interface. Never selects a `careful` item,
/// so the destructive blast radius of a scripted run is bounded by the catalog.
fn clean_headless(mode: Mode, yes: bool) -> anyhow::Result<()> {
    let mut items = collect();
    for it in items.iter_mut() {
        it.selected = it.deletable() && it.risk <= Risk::Rebuildable;
    }

    let picked: Vec<&scan::Item> = items.iter().filter(|i| i.selected).collect();
    let total: u64 = picked.iter().map(|i| i.bytes).sum();

    println!();
    if picked.is_empty() {
        println!("  Nothing to reclaim.");
        println!();
        return Ok(());
    }

    println!(
        "  {} across {} items, safe and rebuildable only.",
        bytes(total),
        picked.len()
    );
    for it in picked.iter().take(12) {
        println!(
            "      {:>9}  {:<9} {}",
            bytes(it.bytes),
            it.risk.label(),
            it.label
        );
    }
    if picked.len() > 12 {
        println!("      {:>9}  and {} more", "", picked.len() - 12);
    }
    println!();

    if mode == Mode::Dry {
        println!("  Dry run — nothing was touched.");
        println!("  Add --trash (recoverable) or --permanent to act.");
        println!();
        return Ok(());
    }

    if !yes {
        let verb = if mode == Mode::Trash {
            "Move all of this to the Trash?"
        } else {
            "Delete all of this permanently? There is no undo."
        };
        print!("  {verb} [y/N] ");
        use std::io::Write;
        std::io::stdout().flush().ok();
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer).ok();
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            println!("  Cancelled.");
            println!();
            return Ok(());
        }
    }

    let out = scan::purge(&items, mode);
    println!();
    println!(
        "  Reclaimed {} from {} items.",
        bytes(out.freed),
        out.removed
    );
    if !out.failed.is_empty() {
        println!("  {} could not be removed:", out.failed.len());
        for (p, e) in out.failed.iter().take(8) {
            println!("      {} — {}", util::tilde(p), e);
        }
    }
    println!("  Free now: {}", bytes(util::disk_usage().free));
    println!();
    Ok(())
}

/// Non-interactive report. Useful over ssh, in a cron job, or piped to a file.
fn headless(verbose: bool) -> anyhow::Result<()> {
    let items = collect();

    let disk = util::disk_usage();
    println!();
    println!(
        "  dustoff — {} used of {}, {} free",
        bytes(disk.used),
        bytes(disk.total),
        bytes(disk.free)
    );
    println!();

    let mut grand = 0u64;
    for cat in rules::CATS {
        let in_cat: Vec<_> = items.iter().filter(|i| i.cat == *cat).collect();
        if in_cat.is_empty() {
            continue;
        }
        let sum: u64 = in_cat.iter().map(|i| i.bytes).sum();
        grand += sum;
        if *cat == Cat::Manual {
            println!("  {} {}", cat.glyph(), cat.title());
        } else {
            println!("  {} {}  —  {}", cat.glyph(), cat.title(), bytes(sum));
        }
        // Advisories are the whole point of their category; never elide them.
        let show = if verbose || *cat == Cat::Manual {
            in_cat.len()
        } else {
            6
        };
        for it in in_cat.iter().take(show) {
            if it.advisory() {
                println!("      {:>9}  {}", "manual", it.label);
                println!("                 $ {}", it.hint);
            } else {
                println!(
                    "      {:>9}  {:<9} {}",
                    bytes(it.bytes),
                    it.risk.label(),
                    it.label
                );
                if verbose {
                    println!("                 {}", util::tilde(&it.path));
                }
            }
        }
        if in_cat.len() > show {
            println!("      {:>9}  and {} more", "", in_cat.len() - show);
        }
        println!();
    }

    let safe: u64 = items
        .iter()
        .filter(|i| i.deletable() && i.risk <= Risk::Rebuildable)
        .map(|i| i.bytes)
        .sum();

    println!("  total found      {}", bytes(grand));
    println!("  safe to reclaim  {}", bytes(safe));
    println!();
    println!("  Run `dustoff` for the interactive view.");
    println!();
    Ok(())
}

fn run(terminal: &mut ratatui::DefaultTerminal, mode: Mode) -> anyhow::Result<()> {
    let mut app = App::new(mode);
    let mut last = Instant::now();

    loop {
        app.pump();
        terminal.draw(|f| ui::draw(f, &mut app))?;

        // Keep the spinner moving while a scan is in flight, but don't burn a
        // core redrawing an idle screen.
        let timeout = if app.scanning {
            Duration::from_millis(80)
        } else {
            Duration::from_millis(250)
        };

        if event::poll(timeout)? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    handle_key(&mut app, k);
                }
            }
        }

        if last.elapsed() >= Duration::from_millis(80) {
            app.tick = app.tick.wrapping_add(1);
            last = Instant::now();
        }

        if app.quit {
            break;
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, k: KeyEvent) {
    // Filter entry swallows most keys while active.
    if app.filtering {
        match k.code {
            KeyCode::Esc => {
                app.filter.clear();
                app.filtering = false;
            }
            KeyCode::Enter => app.filtering = false,
            KeyCode::Backspace => {
                app.filter.pop();
                app.row = 0;
                app.top = 0;
            }
            KeyCode::Char(c) => {
                app.filter.push(c);
                app.row = 0;
                app.top = 0;
            }
            _ => {}
        }
        return;
    }

    if k.modifiers.contains(KeyModifiers::CONTROL) && matches!(k.code, KeyCode::Char('c')) {
        app.quit = true;
        return;
    }

    match app.screen {
        Screen::Confirm => match k.code {
            KeyCode::Char('y') | KeyCode::Enter => app.execute(),
            _ => app.screen = Screen::Browse,
        },

        Screen::Report => match k.code {
            KeyCode::Char('r') => {
                app.screen = Screen::Browse;
                app.rescan();
            }
            KeyCode::Char('q') => app.quit = true,
            _ => app.screen = Screen::Browse,
        },

        Screen::Help => app.screen = Screen::Browse,

        Screen::Working => {}

        Screen::Browse => match k.code {
            KeyCode::Char('q') | KeyCode::Esc => app.quit = true,
            KeyCode::Char('?') => app.screen = Screen::Help,

            KeyCode::Down | KeyCode::Char('j') => match app.pane {
                Pane::Items => app.move_row(1),
                Pane::Cats => app.move_cat(1),
            },
            KeyCode::Up | KeyCode::Char('k') => match app.pane {
                Pane::Items => app.move_row(-1),
                Pane::Cats => app.move_cat(-1),
            },
            KeyCode::PageDown => app.move_row(10),
            KeyCode::PageUp => app.move_row(-10),
            KeyCode::Char('g') => app.row = 0,
            KeyCode::Char('G') => app.move_row(isize::MAX / 2),

            KeyCode::Left | KeyCode::Char('h') => app.pane = Pane::Cats,
            KeyCode::Right | KeyCode::Char('l') => app.pane = Pane::Items,
            KeyCode::Tab => {
                app.pane = match app.pane {
                    Pane::Cats => Pane::Items,
                    Pane::Items => Pane::Cats,
                }
            }

            KeyCode::Char(' ') | KeyCode::Enter => app.toggle(),
            KeyCode::Char('a') => app.toggle_category(),
            KeyCode::Char('A') => app.select_recommended(),
            KeyCode::Char('n') => app.clear_selection(),
            KeyCode::Char('t') => app.toggle_mode(),
            KeyCode::Char('c') => app.copy_hint(),
            KeyCode::Char('/') => {
                app.filtering = true;
                app.filter.clear();
            }
            KeyCode::Char('r') => app.rescan(),

            KeyCode::Char('d') => {
                if app.selected_count() == 0 {
                    app.note("nothing selected — press space, a, or A first");
                } else if app.cat() == Some(Cat::Manual) && app.selected_count() == 0 {
                    app.note("these need a terminal; press c to copy the command");
                } else {
                    app.screen = Screen::Confirm;
                }
            }

            _ => {}
        },
    }
}
