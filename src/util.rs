use std::path::{Path, PathBuf};
use std::process::Command;

/// Format a byte count the way a storage tool should: terse, aligned, honest.
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    if n == 0 {
        return "0".into();
    }
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", n, UNITS[i])
    } else if v >= 100.0 {
        format!("{:.0} {}", v, UNITS[i])
    } else if v >= 10.0 {
        format!("{:.1} {}", v, UNITS[i])
    } else {
        format!("{:.2} {}", v, UNITS[i])
    }
}

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

/// Expand a leading `~` against the real home directory.
pub fn expand(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        home().join(rest)
    } else if s == "~" {
        home()
    } else {
        PathBuf::from(s)
    }
}

/// Render a path for display, collapsing the home prefix back to `~`.
pub fn tilde(p: &Path) -> String {
    let h = home();
    match p.strip_prefix(&h) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

/// Minimal `*` glob: matches `*`, `pre*`, `*suf`, `pre*suf`, and bare literals.
pub fn glob_match(pat: &str, name: &str) -> bool {
    match pat.find('*') {
        None => pat == name,
        Some(i) => {
            let (pre, post) = (&pat[..i], &pat[i + 1..]);
            if !name.starts_with(pre) {
                return false;
            }
            let rest = &name[pre.len()..];
            match post.find('*') {
                None => rest.len() >= post.len() && rest.ends_with(post),
                Some(_) => post
                    .split('*')
                    .filter(|s| !s.is_empty())
                    .all(|frag| rest.contains(frag)),
            }
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct Disk {
    pub total: u64,
    pub used: u64,
    pub free: u64,
}

impl Disk {
    pub fn ratio(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.used as f64 / self.total as f64).clamp(0.0, 1.0)
        }
    }
}

/// Read the boot volume's usage. `df` is the portable answer across macOS and Linux,
/// and avoids pulling libc in just for statvfs.
pub fn disk_usage() -> Disk {
    let out = Command::new("df").args(["-k", "/"]).output();
    let Ok(out) = out else { return Disk::default() };
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() >= 4 {
            let total = f[1].parse::<u64>().unwrap_or(0) * 1024;
            let used = f[2].parse::<u64>().unwrap_or(0) * 1024;
            let free = f[3].parse::<u64>().unwrap_or(0) * 1024;
            if total > 0 {
                return Disk { total, used, free };
            }
        }
    }
    Disk::default()
}

/// Seconds since a path was last modified. `u64::MAX` when unknown, so an
/// "older than N days" filter fails closed and keeps the file.
pub fn age_secs(p: &Path) -> u64 {
    let Ok(md) = std::fs::symlink_metadata(p) else {
        return 0;
    };
    md.modified()
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
