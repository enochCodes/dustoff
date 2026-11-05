//! Talking to a container daemon.
//!
//! On macOS every image, layer and volume lives inside one sparse disk image,
//! so nothing here can be reclaimed by deleting a host path. The daemon has to
//! be asked what it considers reclaimable, and asked to free it.

use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use crate::rules::Risk;

/// One reclaimable bucket reported by `docker system df`.
pub struct Reclaim {
    pub label: &'static str,
    pub detail: &'static str,
    pub risk: Risk,
    pub argv: &'static [&'static str],
    pub bytes: u64,
}

/// Run a command and capture stdout, giving up after `secs`.
///
/// A stopped or starting Docker daemon can leave the CLI hanging, and a storage
/// tool must not freeze on a scan because of it. The command is spawned on a
/// helper thread; on timeout we stop waiting and move on.
fn capture(argv: &[&str], secs: u64) -> Option<String> {
    let owned: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let out = Command::new(&owned[0])
            .args(&owned[1..])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        let _ = tx.send(out);
    });

    match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(Ok(out)) if out.status.success() => Some(String::from_utf8_lossy(&out.stdout).into()),
        _ => None,
    }
}

/// Parse a size as the Docker CLI prints it: `8.123GB`, `900kB`, `0B`, `1.5MiB`.
///
/// Docker formats with base-1000 units (`kB`, `MB`, `GB`); the binary `KiB`
/// spellings are accepted too in case a future version switches.
pub fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let split = s
        .find(|c: char| !c.is_ascii_digit() && c != '.' && c != ',')
        .unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let num: f64 = num.replace(',', "").parse().ok()?;

    let mult: f64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "kb" | "k" => 1_000.0,
        "mb" | "m" => 1_000_000.0,
        "gb" | "g" => 1_000_000_000.0,
        "tb" | "t" => 1_000_000_000_000.0,
        "kib" => 1024.0,
        "mib" => 1024.0 * 1024.0,
        "gib" => 1024.0 * 1024.0 * 1024.0,
        "tib" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    // Truncation would turn 8.123GB into 8122999999; the figure is an
    // approximation already, so round rather than floor.
    Some((num * mult).round() as u64)
}

/// The four buckets `docker system df` reports, with the command that frees each.
///
/// The reclaimable figure Docker prints for Images and Local Volumes counts
/// everything unused, which is what `-a` removes — so the number shown and the
/// command offered agree.
fn bucket(kind: &str, bytes: u64) -> Option<Reclaim> {
    let (label, detail, risk, argv): (_, _, _, &'static [&'static str]) = match kind {
        "Images" => (
            "Docker · unused images",
            "Images not used by any container. Each one has to be pulled or built \
             again, which can be slow on a bad connection.",
            Risk::Careful,
            &["docker", "image", "prune", "-a", "-f"],
        ),
        "Containers" => (
            "Docker · stopped containers",
            "Containers that have exited. Their writable layers go with them; \
             named volumes they used are not touched.",
            Risk::Rebuildable,
            &["docker", "container", "prune", "-f"],
        ),
        "Local Volumes" => (
            "Docker · unused volumes",
            "Volumes no container references. This is where databases live — if a \
             container was removed but its data was meant to outlive it, this \
             deletes that data. Never selected automatically.",
            Risk::Careful,
            &["docker", "volume", "prune", "-a", "-f"],
        ),
        "Build Cache" => (
            "Docker · build cache",
            "Intermediate build layers. Usually the largest bucket and the safest \
             to clear; the next build is slower and nothing else changes.",
            Risk::Rebuildable,
            &["docker", "builder", "prune", "-a", "-f"],
        ),
        _ => return None,
    };
    Some(Reclaim {
        label,
        detail,
        risk,
        argv,
        bytes,
    })
}

/// Parse the table `docker system df` prints.
///
/// The `--format` field names are not documented, so the default table is the
/// stable surface. Columns are TYPE, TOTAL, ACTIVE, SIZE, RECLAIMABLE, and two
/// of the type names contain a space, so rows are matched by known prefix.
pub fn parse_df(table: &str) -> Vec<Reclaim> {
    const KINDS: [&str; 4] = ["Images", "Containers", "Local Volumes", "Build Cache"];
    let mut out = Vec::new();

    for line in table.lines() {
        let line = line.trim_end();
        let Some(kind) = KINDS.iter().find(|k| line.starts_with(**k)) else {
            continue;
        };
        let rest: Vec<&str> = line[kind.len()..].split_whitespace().collect();
        // TOTAL, ACTIVE, SIZE, RECLAIMABLE — anything shorter is a format we
        // do not recognise, and a wrong number is worse than no number.
        if rest.len() < 4 {
            continue;
        }
        let Some(bytes) = parse_size(rest[3]) else {
            continue;
        };
        if bytes == 0 {
            continue;
        }
        if let Some(r) = bucket(kind, bytes) {
            out.push(r);
        }
    }
    out
}

/// Ask Docker what it can free. Empty when Docker is missing, stopped, or slow.
pub fn reclaimable() -> Vec<Reclaim> {
    match capture(&["docker", "system", "df"], 6) {
        Some(table) => parse_df(&table),
        None => Vec::new(),
    }
}

/// Run one prune. `argv` is a compile-time constant from the catalog and is
/// passed straight to exec — there is no shell and nothing is interpolated.
pub fn run(argv: &[&str], secs: u64) -> Result<(), String> {
    if argv.is_empty() {
        return Err("no command".into());
    }
    match capture(argv, secs) {
        Some(_) => Ok(()),
        None => Err(format!("`{}` failed or timed out", argv.join(" "))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_docker_sizes() {
        assert_eq!(parse_size("0B"), Some(0));
        assert_eq!(parse_size("900kB"), Some(900_000));
        assert_eq!(parse_size("1.5MB"), Some(1_500_000));
        assert_eq!(parse_size("8.123GB"), Some(8_123_000_000));
        assert_eq!(parse_size("2TB"), Some(2_000_000_000_000));
        assert_eq!(parse_size("1MiB"), Some(1_048_576));
        assert_eq!(parse_size("12"), Some(12));
        assert_eq!(parse_size("whatever"), None);
        assert_eq!(parse_size(""), None);
    }

    const REAL_TABLE: &str = "\
TYPE            TOTAL     ACTIVE    SIZE      RECLAIMABLE
Images          23        4         12.34GB   8.123GB (65%)
Containers      7         2         1.2MB     900kB (75%)
Local Volumes   11        3         4.5GB     3.2GB (71%)
Build Cache     142       0         6.7GB     6.7GB
";

    #[test]
    fn parses_the_df_table() {
        let rows = parse_df(REAL_TABLE);
        assert_eq!(rows.len(), 4);

        let images = &rows[0];
        assert_eq!(images.label, "Docker · unused images");
        assert_eq!(images.bytes, 8_123_000_000);
        assert_eq!(images.risk, Risk::Careful);
        assert_eq!(images.argv, &["docker", "image", "prune", "-a", "-f"]);

        let cache = rows
            .iter()
            .find(|r| r.label.contains("build cache"))
            .unwrap();
        assert_eq!(cache.bytes, 6_700_000_000);
        assert_eq!(cache.risk, Risk::Rebuildable);
    }

    #[test]
    fn volumes_are_never_auto_selected() {
        let rows = parse_df(REAL_TABLE);
        let vols = rows.iter().find(|r| r.label.contains("volumes")).unwrap();
        // The whole point: `--clean` and `A` only take Safe and Rebuildable.
        assert_eq!(vols.risk, Risk::Careful);
    }

    #[test]
    fn empty_buckets_are_dropped() {
        let table = "\
TYPE            TOTAL     ACTIVE    SIZE      RECLAIMABLE
Images          0         0         0B        0B
Build Cache     3         0         1GB       1GB
";
        let rows = parse_df(table);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].label.contains("build cache"));
    }

    #[test]
    fn garbage_in_yields_nothing_rather_than_wrong_numbers() {
        assert!(parse_df("").is_empty());
        assert!(parse_df("Cannot connect to the Docker daemon").is_empty());
        assert!(parse_df("Images  broken").is_empty());
        assert!(parse_df("Images 1 2 3 notasize").is_empty());
    }

    #[test]
    fn missing_binary_times_out_cleanly() {
        assert!(capture(&["definitely-not-a-real-binary-9f3a"], 2).is_none());
        assert!(run(&[], 1).is_err());
    }
}
