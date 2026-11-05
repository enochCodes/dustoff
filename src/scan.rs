//! Resolving rules to real paths, sizing them, and removing them.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::danger::{vet, vet_dir, vet_search_root};
use crate::rules::{Act, Cat, Finder, Risk, Rule};
use crate::util::{age_secs, expand, glob_match};

#[derive(Clone, Debug)]
pub struct Item {
    pub cat: Cat,
    pub label: &'static str,
    pub detail: &'static str,
    pub hint: &'static str,
    pub risk: Risk,
    pub act: Act,
    pub path: PathBuf,
    pub bytes: u64,
    pub files: u64,
    pub selected: bool,
    /// Set when the safety layer refused the path. Shown, never deletable.
    pub blocked: Option<String>,
}

impl Item {
    pub fn advisory(&self) -> bool {
        self.act == Act::Advise
    }
    pub fn deletable(&self) -> bool {
        self.blocked.is_none() && !self.advisory() && self.bytes > 0
    }
}

pub enum Msg {
    Probing(String),
    Found(Box<Item>),
    Done,
}

/// Apparent size of a tree. Symlinks are never followed, so a link into
/// a huge directory is counted as the link itself.
fn measure(path: &Path, cancel: &AtomicBool) -> (u64, u64) {
    let md = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return (0, 0),
    };
    if md.file_type().is_symlink() {
        return (md.len(), 1);
    }
    if md.is_file() {
        return (md.len(), 1);
    }
    let mut bytes = 0u64;
    let mut files = 0u64;
    for e in WalkDir::new(path).follow_links(false).into_iter() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(e) = e else { continue };
        if let Ok(m) = e.metadata() {
            if m.is_file() || m.file_type().is_symlink() {
                bytes += m.len();
                files += 1;
            }
        }
    }
    (bytes, files)
}

/// Turn one rule into the concrete paths it refers to on this machine.
fn resolve(rule: &Rule) -> Vec<PathBuf> {
    match &rule.find {
        Finder::At(p) => {
            let path = expand(p);
            if !path.exists() {
                return vec![];
            }
            match rule.act {
                Act::Each => read_children(&path),
                // Dir and Clear are both a single row; they differ only in
                // whether the folder itself survives the delete.
                _ => vec![path],
            }
        }

        Finder::Glob { base, pat } => {
            let base = expand(base);
            let Ok(rd) = fs::read_dir(&base) else {
                return vec![];
            };
            rd.filter_map(|e| e.ok())
                .filter(|e| glob_match(pat, &e.file_name().to_string_lossy()))
                .map(|e| e.path())
                .collect()
        }

        Finder::Named {
            roots,
            names,
            depth,
            older_days,
        } => {
            let cutoff = older_days * 86_400;
            let mut out = Vec::new();
            for root in *roots {
                let root = expand(root);
                if !root.is_dir() || !vet_search_root(&root).is_allow() {
                    continue;
                }
                for e in WalkDir::new(&root)
                    .follow_links(false)
                    .min_depth(1)
                    .max_depth(*depth)
                    .into_iter()
                    .flatten()
                {
                    let n = e.file_name().to_string_lossy().to_string();
                    if e.file_type().is_dir()
                        && names.contains(&n.as_str())
                        && (cutoff == 0 || age_secs(e.path()) >= cutoff)
                    {
                        out.push(e.path().to_path_buf());
                    }
                }
            }
            // A node_modules inside a node_modules is not a separate finding.
            // Sorting puts every parent ahead of its children.
            out.sort();
            let mut top: Vec<PathBuf> = Vec::with_capacity(out.len());
            for p in out {
                if !top.iter().any(|k| p.starts_with(k)) {
                    top.push(p);
                }
            }
            top
        }

        Finder::Files {
            base,
            exts,
            depth,
            min_mb,
            older_days,
        } => {
            let base = expand(base);
            if !base.is_dir() || !vet_search_root(&base).is_allow() {
                return vec![];
            }
            let min = min_mb * 1024 * 1024;
            let cutoff = older_days * 86_400;
            WalkDir::new(&base)
                .follow_links(false)
                .min_depth(1)
                .max_depth(*depth)
                .into_iter()
                .flatten()
                .filter(|e| e.file_type().is_file())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|x| {
                            let x = x.to_string_lossy().to_lowercase();
                            exts.iter().any(|w| *w == x)
                        })
                        .unwrap_or(false)
                })
                .filter(|e| e.metadata().map(|m| m.len() >= min).unwrap_or(false))
                .filter(|e| cutoff == 0 || age_secs(e.path()) >= cutoff)
                .map(|e| e.path().to_path_buf())
                .collect()
        }

        Finder::Litter {
            roots,
            names,
            depth,
        } => {
            let mut out = Vec::new();
            for root in *roots {
                let root = expand(root);
                if !root.is_dir() || !vet_search_root(&root).is_allow() {
                    continue;
                }
                for e in WalkDir::new(&root)
                    .follow_links(false)
                    .min_depth(1)
                    .max_depth(*depth)
                    .into_iter()
                    .flatten()
                {
                    let n = e.file_name().to_string_lossy().to_string();
                    if e.file_type().is_file() && names.contains(&n.as_str()) {
                        out.push(e.path().to_path_buf());
                    }
                }
            }
            out
        }
    }
}

fn read_children(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default()
}

/// Walk the whole catalog, streaming results so the interface fills as it goes.
pub fn run(rules: Vec<Rule>, tx: Sender<Msg>, cancel: Arc<AtomicBool>) {
    // Advisories first: they cost nothing to produce and give the UI content
    // immediately while the real scan warms up.
    for rule in rules.iter().filter(|r| r.act == Act::Advise) {
        let _ = tx.send(Msg::Found(Box::new(Item {
            cat: Cat::Manual,
            label: rule.label,
            detail: rule.detail,
            hint: rule.hint,
            risk: rule.risk,
            act: Act::Advise,
            path: PathBuf::new(),
            bytes: 0,
            files: 0,
            selected: false,
            blocked: None,
        })));
    }

    let scannable: Vec<&Rule> = rules.iter().filter(|r| r.act != Act::Advise).collect();

    scannable.par_iter().for_each(|rule| {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let _ = tx.send(Msg::Probing(rule.label.to_string()));

        let act = match rule.find {
            Finder::At(_) => rule.act,
            _ => Act::Item,
        };

        for path in resolve(rule) {
            if cancel.load(Ordering::Relaxed) {
                return;
            }

            // Every candidate is vetted here, regardless of what the rule claimed.
            let verdict = match act {
                Act::Dir => vet_dir(&path),
                Act::Clear => vet_search_root(&path),
                _ => vet(&path),
            };
            let blocked = verdict.reason().map(str::to_string);

            let (bytes, files) = if blocked.is_some() {
                (0, 0)
            } else {
                measure(&path, &cancel)
            };

            if bytes == 0 && blocked.is_none() {
                continue; // nothing there; don't clutter the list
            }

            let _ = tx.send(Msg::Found(Box::new(Item {
                cat: rule.cat,
                label: rule.label,
                detail: rule.detail,
                hint: rule.hint,
                risk: rule.risk,
                act,
                path,
                bytes,
                files,
                selected: false,
                blocked,
            })));
        }
    });

    let _ = tx.send(Msg::Done);
}

#[derive(Default, Debug)]
pub struct Outcome {
    pub freed: u64,
    pub removed: usize,
    pub failed: Vec<(PathBuf, String)>,
}

/// Clear the read-only bits Go's module cache sets, so removal can proceed.
#[cfg(unix)]
fn make_writable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    for e in WalkDir::new(path).follow_links(false).into_iter().flatten() {
        if let Ok(md) = e.metadata() {
            let mut p = md.permissions();
            p.set_mode(p.mode() | 0o200);
            let _ = fs::set_permissions(e.path(), p);
        }
    }
}

#[cfg(not(unix))]
fn make_writable(_path: &Path) {}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Report only. Nothing is touched.
    Dry,
    /// Move to the system Trash, recoverable.
    Trash,
    /// Unlink immediately.
    Permanent,
}

/// Remove the selected items. Re-vets every path immediately before acting —
/// the list may have been built minutes ago.
pub fn purge(items: &[Item], mode: Mode) -> Outcome {
    let mut out = Outcome::default();

    for it in items.iter().filter(|i| i.selected && i.deletable()) {
        let verdict = match it.act {
            Act::Dir => vet_dir(&it.path),
            Act::Clear => vet_search_root(&it.path),
            _ => vet(&it.path),
        };
        if let Some(reason) = verdict.reason() {
            out.failed.push((it.path.clone(), reason.to_string()));
            continue;
        }
        if !it.path.exists() {
            continue;
        }

        if mode == Mode::Dry {
            out.freed += it.bytes;
            out.removed += 1;
            continue;
        }

        if it.act == Act::Clear {
            if !it.path.is_dir() {
                out.failed
                    .push((it.path.clone(), "expected a directory to clear".into()));
                continue;
            }
            let mut cleared = 0u64;
            for child in read_children(&it.path) {
                if !vet(&child).is_allow() {
                    continue;
                }
                let r = remove_one(&child, mode);
                match r {
                    Ok(()) => cleared += 1,
                    Err(e) => out.failed.push((child, e)),
                }
            }
            if cleared > 0 {
                out.freed += it.bytes;
                out.removed += 1;
            }
            continue;
        }

        match remove_one(&it.path, mode) {
            Ok(()) => {
                out.freed += it.bytes;
                out.removed += 1;
            }
            Err(e) => out.failed.push((it.path.clone(), e)),
        }
    }

    out
}

fn remove_one(path: &Path, mode: Mode) -> Result<(), String> {
    if mode == Mode::Trash {
        return trash::delete(path).map_err(|e| e.to_string());
    }
    let is_dir = fs::symlink_metadata(path)
        .map(|m| m.is_dir())
        .unwrap_or(false);
    if is_dir {
        make_writable(path);
        fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::Finder;

    #[test]
    fn measure_counts_a_known_tree() {
        let dir = std::env::temp_dir().join(format!("dustoff-test-{}", std::process::id()));
        let _ = fs::create_dir_all(dir.join("nested"));
        fs::write(dir.join("a.bin"), vec![0u8; 2048]).unwrap();
        fs::write(dir.join("nested/b.bin"), vec![0u8; 1024]).unwrap();

        let cancel = AtomicBool::new(false);
        let (bytes, files) = measure(&dir, &cancel);
        assert_eq!(bytes, 3072);
        assert_eq!(files, 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_paths_resolve_to_nothing() {
        let rule = Rule {
            cat: Cat::Dev,
            label: "x",
            detail: "",
            risk: Risk::Safe,
            act: Act::Dir,
            find: Finder::At("~/definitely-not-a-real-directory-9f3a"),
            hint: "",
        };
        assert!(resolve(&rule).is_empty());
    }

    #[test]
    fn dry_run_removes_nothing() {
        // Must live under $HOME: purge() re-vets every path, and anything
        // outside $HOME is refused by design.
        let dir = crate::util::home().join(format!(".dustoff-dry-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("keep.bin"), vec![0u8; 512]).unwrap();

        let it = Item {
            cat: Cat::Dev,
            label: "x",
            detail: "",
            hint: "",
            risk: Risk::Safe,
            act: Act::Dir,
            path: dir.join("keep.bin"),
            bytes: 512,
            files: 1,
            selected: true,
            blocked: None,
        };
        let out = purge(std::slice::from_ref(&it), Mode::Dry);
        assert_eq!(out.freed, 512);
        assert!(dir.join("keep.bin").exists(), "dry run must not delete");

        let _ = fs::remove_dir_all(&dir);
    }

    fn item(path: PathBuf, act: Act, bytes: u64) -> Item {
        Item {
            cat: Cat::Dev,
            label: "x",
            detail: "",
            hint: "",
            risk: Risk::Safe,
            act,
            path,
            bytes,
            files: 1,
            selected: true,
            blocked: None,
        }
    }

    #[test]
    fn clear_empties_the_folder_but_keeps_it() {
        let dir = crate::util::home().join(format!(".dustoff-clear-{}", std::process::id()));
        fs::create_dir_all(dir.join("sub/deep")).unwrap();
        fs::write(dir.join("a.bin"), vec![0u8; 100]).unwrap();
        fs::write(dir.join("sub/deep/b.bin"), vec![0u8; 100]).unwrap();

        let out = purge(&[item(dir.clone(), Act::Clear, 200)], Mode::Permanent);
        assert!(
            out.failed.is_empty(),
            "unexpected failures: {:?}",
            out.failed
        );
        assert!(dir.exists(), "Clear must keep the folder itself");
        assert_eq!(
            fs::read_dir(&dir).unwrap().count(),
            0,
            "contents should be gone"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn removes_read_only_trees_like_the_go_module_cache() {
        use std::os::unix::fs::PermissionsExt;
        let dir = crate::util::home().join(format!(".dustoff-ro-{}", std::process::id()));
        fs::create_dir_all(dir.join("mod/pkg")).unwrap();
        fs::write(dir.join("mod/pkg/src.go"), vec![0u8; 64]).unwrap();
        // Exactly what `go mod download` leaves behind: everything read-only.
        for p in [
            dir.join("mod/pkg/src.go"),
            dir.join("mod/pkg"),
            dir.join("mod"),
        ] {
            let mut perm = fs::metadata(&p).unwrap().permissions();
            perm.set_mode(0o555);
            fs::set_permissions(&p, perm).unwrap();
        }

        let out = purge(&[item(dir.join("mod"), Act::Dir, 64)], Mode::Permanent);
        assert!(
            out.failed.is_empty(),
            "unexpected failures: {:?}",
            out.failed
        );
        assert!(
            !dir.join("mod").exists(),
            "read-only tree should be removed"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_to_act_outside_home_even_when_asked() {
        let bait = std::env::temp_dir().join(format!("dustoff-bait-{}", std::process::id()));
        fs::create_dir_all(&bait).unwrap();
        fs::write(bait.join("precious.bin"), vec![0u8; 32]).unwrap();

        let out = purge(&[item(bait.clone(), Act::Dir, 32)], Mode::Permanent);
        assert_eq!(out.removed, 0);
        assert_eq!(out.failed.len(), 1);
        assert!(bait.join("precious.bin").exists(), "must not touch /tmp");

        let _ = fs::remove_dir_all(&bait);
    }
}
