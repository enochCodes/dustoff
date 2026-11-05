use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use crate::rules::{CATS, Cat, Risk};
use crate::scan::{self, Item, Mode, Msg, Outcome};
use crate::util::{self, Disk};

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Screen {
    Browse,
    Confirm,
    Working,
    Report,
    Help,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Pane {
    Cats,
    Items,
}

pub struct App {
    pub items: Vec<Item>,
    pub cat_idx: usize,
    pub row: usize,
    pub top: usize,
    pub pane: Pane,
    pub screen: Screen,
    pub mode: Mode,
    pub disk: Disk,

    pub scanning: bool,
    pub probing: String,
    pub tick: usize,
    pub started: Instant,

    pub filter: String,
    pub filtering: bool,

    pub outcome: Option<Outcome>,
    pub toast: Option<(String, Instant)>,

    rx: Option<Receiver<Msg>>,
    cancel: Arc<AtomicBool>,
    pub quit: bool,
}

impl App {
    pub fn new(mode: Mode) -> Self {
        let mut app = Self::blank(mode);
        app.rescan();
        app
    }

    /// An app with no scan running. `new` builds on this; tests render from it.
    pub fn blank(mode: Mode) -> Self {
        Self {
            items: Vec::new(),
            cat_idx: 0,
            row: 0,
            top: 0,
            pane: Pane::Items,
            screen: Screen::Browse,
            mode,
            disk: util::disk_usage(),
            scanning: false,
            probing: String::new(),
            tick: 0,
            started: Instant::now(),
            filter: String::new(),
            filtering: false,
            outcome: None,
            toast: None,
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
            quit: false,
        }
    }

    pub fn rescan(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.items.clear();
        self.row = 0;
        self.top = 0;
        self.outcome = None;
        self.disk = util::disk_usage();
        self.started = Instant::now();

        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = cancel.clone();
        let (tx, rx) = channel();
        self.rx = Some(rx);
        self.scanning = true;
        self.probing = "starting".into();

        std::thread::spawn(move || {
            scan::run(crate::rules::catalog(), tx, cancel);
        });
    }

    /// Drain whatever the scanner has produced since the last frame.
    pub fn pump(&mut self) {
        let mut done = false;
        if let Some(rx) = &self.rx {
            for _ in 0..512 {
                match rx.try_recv() {
                    Ok(Msg::Probing(l)) => self.probing = l,
                    Ok(Msg::Found(it)) => self.items.push(*it),
                    Ok(Msg::Done) => {
                        done = true;
                        break;
                    }
                    Err(_) => break,
                }
            }
        }
        if done {
            self.scanning = false;
            self.probing.clear();
            self.rx = None;
            self.sort();
        }
    }

    fn sort(&mut self) {
        self.items.sort_by(|a, b| {
            let ai = CATS.iter().position(|c| *c == a.cat).unwrap_or(99);
            let bi = CATS.iter().position(|c| *c == b.cat).unwrap_or(99);
            ai.cmp(&bi).then(b.bytes.cmp(&a.bytes))
        });
    }

    // ── derived views ────────────────────────────────────────────────────

    /// Categories that actually have findings, in catalog order.
    pub fn cats(&self) -> Vec<Cat> {
        CATS.iter()
            .copied()
            .filter(|c| self.items.iter().any(|i| i.cat == *c))
            .collect()
    }

    pub fn cat(&self) -> Option<Cat> {
        self.cats().get(self.cat_idx).copied()
    }

    /// Indices into `self.items` for the current category and filter.
    pub fn visible(&self) -> Vec<usize> {
        let Some(cat) = self.cat() else { return vec![] };
        let f = self.filter.to_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.cat == cat)
            .filter(|(_, i)| {
                f.is_empty()
                    || i.label.to_lowercase().contains(&f)
                    || i.path.to_string_lossy().to_lowercase().contains(&f)
            })
            .map(|(n, _)| n)
            .collect()
    }

    pub fn current(&self) -> Option<&Item> {
        self.visible().get(self.row).map(|n| &self.items[*n])
    }

    pub fn cat_bytes(&self, c: Cat) -> u64 {
        self.items
            .iter()
            .filter(|i| i.cat == c)
            .map(|i| i.bytes)
            .sum()
    }

    pub fn cat_selected_bytes(&self, c: Cat) -> u64 {
        self.items
            .iter()
            .filter(|i| i.cat == c && i.selected)
            .map(|i| i.bytes)
            .sum()
    }

    pub fn total_bytes(&self) -> u64 {
        self.items.iter().map(|i| i.bytes).sum()
    }

    pub fn selected_bytes(&self) -> u64 {
        self.items
            .iter()
            .filter(|i| i.selected)
            .map(|i| i.bytes)
            .sum()
    }

    pub fn selected_count(&self) -> usize {
        self.items.iter().filter(|i| i.selected).count()
    }

    pub fn blocked_count(&self) -> usize {
        self.items.iter().filter(|i| i.blocked.is_some()).count()
    }

    /// Largest single item in a category, for scaling the bars.
    pub fn cat_peak(&self, c: Cat) -> u64 {
        self.items
            .iter()
            .filter(|i| i.cat == c)
            .map(|i| i.bytes)
            .max()
            .unwrap_or(1)
            .max(1)
    }

    // ── movement ─────────────────────────────────────────────────────────

    pub fn move_row(&mut self, delta: isize) {
        let n = self.visible().len();
        if n == 0 {
            self.row = 0;
            return;
        }
        let next = self.row as isize + delta;
        self.row = next.clamp(0, n as isize - 1) as usize;
    }

    pub fn move_cat(&mut self, delta: isize) {
        let n = self.cats().len();
        if n == 0 {
            return;
        }
        let next = self.cat_idx as isize + delta;
        self.cat_idx = next.clamp(0, n as isize - 1) as usize;
        self.row = 0;
        self.top = 0;
    }

    pub fn clamp_scroll(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.row < self.top {
            self.top = self.row;
        } else if self.row >= self.top + height {
            self.top = self.row + 1 - height;
        }
    }

    // ── selection ────────────────────────────────────────────────────────

    pub fn toggle(&mut self) {
        if let Some(&n) = self.visible().get(self.row) {
            if self.items[n].deletable() {
                self.items[n].selected = !self.items[n].selected;
            } else if self.items[n].advisory() {
                self.note("that one needs a terminal — press c to copy its command");
            } else if let Some(r) = self.items[n].blocked.clone() {
                self.note(&format!("blocked: {r}"));
            }
        }
    }

    pub fn toggle_category(&mut self) {
        let idx = self.visible();
        let all_on = idx
            .iter()
            .filter(|n| self.items[**n].deletable())
            .all(|n| self.items[*n].selected);
        let mut touched = 0;
        for n in idx {
            if self.items[n].deletable() {
                self.items[n].selected = !all_on;
                touched += 1;
            }
        }
        if touched == 0 {
            self.note("nothing selectable in this category");
        }
    }

    /// Select everything the catalog marks Safe or Rebuildable, everywhere.
    /// Deliberately never touches Careful items.
    pub fn select_recommended(&mut self) {
        let mut n = 0;
        for it in self.items.iter_mut() {
            if it.deletable() && it.risk <= Risk::Rebuildable {
                it.selected = true;
                n += 1;
            }
        }
        self.note(&format!("selected {n} safe and rebuildable items"));
    }

    pub fn clear_selection(&mut self) {
        for it in self.items.iter_mut() {
            it.selected = false;
        }
        self.note("selection cleared");
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            Mode::Dry => Mode::Trash,
            Mode::Trash => Mode::Permanent,
            Mode::Permanent => Mode::Dry,
        };
        self.note(match self.mode {
            Mode::Dry => "dry run — nothing will be removed",
            Mode::Trash => "move to Trash — recoverable",
            Mode::Permanent => "permanent delete — not recoverable",
        });
    }

    pub fn mode_label(&self) -> &'static str {
        match self.mode {
            Mode::Dry => "dry run",
            Mode::Trash => "trash",
            Mode::Permanent => "permanent",
        }
    }

    // ── actions ──────────────────────────────────────────────────────────

    pub fn execute(&mut self) {
        self.screen = Screen::Working;
        let out = scan::purge(&self.items, self.mode);

        if self.mode != Mode::Dry {
            // Drop what actually went, keep what failed so the report is honest.
            let failed: Vec<PathBuf> = out.failed.iter().map(|(p, _)| p.clone()).collect();
            self.items
                .retain(|i| !i.selected || !i.deletable() || failed.contains(&i.path));
            for it in self.items.iter_mut() {
                it.selected = false;
            }
            self.disk = util::disk_usage();
        } else {
            for it in self.items.iter_mut() {
                it.selected = false;
            }
        }

        self.row = 0;
        self.top = 0;
        self.outcome = Some(out);
        self.screen = Screen::Report;
    }

    /// Put an advisory's command on the clipboard when the platform offers one.
    pub fn copy_hint(&mut self) {
        let Some(hint) = self.current().map(|i| i.hint.to_string()) else {
            return;
        };
        if hint.is_empty() {
            self.note("no command for this item");
            return;
        }
        if copy_to_clipboard(&hint) {
            self.note("command copied to clipboard");
        } else {
            self.note(&hint);
        }
    }

    pub fn note(&mut self, msg: &str) {
        self.toast = Some((msg.to_string(), Instant::now()));
    }

    pub fn toast_text(&self) -> Option<&str> {
        self.toast.as_ref().and_then(|(m, t)| {
            if t.elapsed() < Duration::from_secs(4) {
                Some(m.as_str())
            } else {
                None
            }
        })
    }
}

fn copy_to_clipboard(text: &str) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};

    for prog in ["pbcopy", "wl-copy", "xclip"] {
        let mut cmd = Command::new(prog);
        if prog == "xclip" {
            cmd.args(["-selection", "clipboard"]);
        }
        if let Ok(mut child) = cmd.stdin(Stdio::piped()).stdout(Stdio::null()).spawn() {
            if let Some(mut si) = child.stdin.take() {
                let _ = si.write_all(text.as_bytes());
            }
            if child.wait().map(|s| s.success()).unwrap_or(false) {
                return true;
            }
        }
    }
    false
}
