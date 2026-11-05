//! The only module that decides whether a path may be touched.
//!
//! Everything destructive in this program routes through [`vet`]. The rule
//! catalog is data and could be wrong; this is the backstop that assumes it is.

use std::path::{Component, Path};

use crate::util::home;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny(String),
}

impl Verdict {
    pub fn is_allow(&self) -> bool {
        matches!(self, Verdict::Allow)
    }
    pub fn reason(&self) -> Option<&str> {
        match self {
            Verdict::Deny(r) => Some(r),
            Verdict::Allow => None,
        }
    }
}

/// Path fragments that are never deletable, matched case-insensitively anywhere
/// in the path. Credentials, keys, and the folders that hold a person's actual
/// work. A cache rule that wanders into one of these is a bug, and this catches it.
const NEVER: &[&str] = &[
    "/.ssh",
    "/.gnupg",
    "/.pgp",
    "/.aws",
    "/.kube",
    "/.netrc",
    "/.authinfo",
    "/.password-store",
    "/.mcp-auth",
    "/library/keychains",
    "/keychain",
    "/credentials",
    "/.credentials.json",
    "/.config/gh/hosts.yml",
    "/.docker/config.json",
    "/.gitconfig",
    "/.git-credentials",
    "/.npmrc",
    "/.pypirc",
    "/.cargo/credentials",
    "/library/mobile documents",
    "/library/application support/addressbook",
    "/library/application support/com.apple.sharedfilelist",
    "/library/calendars",
    "/library/messages",
    "/library/mail/v10/mailboxes",
    "/library/photos",
    "/.gnupg",
    "/.local/share/keyrings",
    "id_rsa",
    "id_ed25519",
    ".pem",
    ".p12",
    ".keystore",
    ".jks",
];

/// Directories under $HOME that hold the user's own files. A rule may only reach
/// inside these for narrowly-typed finders (specific extensions, specific names),
/// never for a whole-directory delete.
const USER_DATA_ROOTS: &[&str] = &[
    "Documents",
    "Desktop",
    "Pictures",
    "Movies",
    "Music",
    "Public",
    "Applications",
    "Library/Keychains",
];

/// Check one path against every rail. Returns the reason on refusal so the UI
/// can show it rather than silently dropping the item.
///
/// `min_depth` is how many components below $HOME a path must sit. Deleting
/// needs 2 (`~/Library/Caches/x`, never `~/Library`); merely *searching* a
/// directory for typed matches needs only 1 (`~/Downloads`).
fn vet_inner(path: &Path, min_depth: usize) -> Verdict {
    let deny = |m: &str| Verdict::Deny(m.to_string());

    if path.as_os_str().is_empty() {
        return deny("empty path");
    }
    if !path.is_absolute() {
        return deny("not an absolute path");
    }

    // No `..` anywhere. We never resolve traversal; we refuse it.
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return deny("contains '..'");
    }

    let home = home();
    if home.as_os_str().is_empty() || home == Path::new("/") {
        return deny("home directory could not be determined");
    }

    // Never the roots themselves.
    if path == home || path == Path::new("/") {
        return deny("refusing to target a root directory");
    }
    for top in [
        "/System",
        "/usr",
        "/bin",
        "/sbin",
        "/etc",
        "/var",
        "/Applications",
    ] {
        if path == Path::new(top) {
            return deny("refusing to target a system directory");
        }
    }

    // Containment: only inside $HOME or a mounted volume.
    let in_home = path.starts_with(&home);
    let in_volumes = path.starts_with("/Volumes") && path.components().count() > 3;
    if !in_home && !in_volumes {
        return deny("outside $HOME and /Volumes");
    }

    // Depth guard: `~/Library` or `~/.cargo` alone is too coarse to delete.
    if in_home {
        let depth = path
            .strip_prefix(&home)
            .map(|r| r.components().count())
            .unwrap_or(0);
        if depth < min_depth {
            return deny("too close to $HOME — needs a more specific path");
        }
    }

    let lower = path.to_string_lossy().to_lowercase();
    for frag in NEVER {
        if lower.contains(frag) {
            return deny(&format!(
                "protected location ({})",
                frag.trim_start_matches('/')
            ));
        }
    }

    Verdict::Allow
}

/// Vet a path for deletion.
pub fn vet(path: &Path) -> Verdict {
    vet_inner(path, 2)
}

/// Stricter variant for whole-directory removal: additionally refuses anything
/// inside the user's own document folders, however the rule phrased it.
pub fn vet_dir(path: &Path) -> Verdict {
    let v = vet(path);
    if !v.is_allow() {
        return v;
    }
    let home = home();
    for root in USER_DATA_ROOTS {
        let r = home.join(root);
        if path == r || path.starts_with(&r) {
            return Verdict::Deny(format!("inside ~/{root} — your own files live there"));
        }
    }
    Verdict::Allow
}

/// Roots a typed finder (extensions, exact names) is allowed to search.
/// Deliberately broader than `vet_dir`: finding `*.dmg` in ~/Downloads is fine,
/// deleting ~/Downloads is not.
pub fn vet_search_root(path: &Path) -> Verdict {
    vet_inner(path, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn h(sub: &str) -> PathBuf {
        home().join(sub)
    }

    #[test]
    fn refuses_traversal_and_roots() {
        assert!(!vet(Path::new("relative/path")).is_allow());
        assert!(!vet(&h("Library/../.ssh")).is_allow());
        assert!(!vet(&home()).is_allow());
        assert!(!vet(Path::new("/")).is_allow());
        assert!(!vet(Path::new("/System")).is_allow());
    }

    #[test]
    fn refuses_secrets() {
        assert!(!vet(&h(".ssh/id_rsa")).is_allow());
        assert!(!vet(&h(".aws/credentials")).is_allow());
        assert!(!vet(&h("Library/Keychains/login.keychain-db")).is_allow());
        assert!(!vet(&h(".claude/.credentials.json")).is_allow());
        assert!(!vet(&h(".mcp-auth/tokens.json")).is_allow());
    }

    #[test]
    fn refuses_shallow_and_outside() {
        assert!(!vet(&h("Library")).is_allow());
        assert!(!vet(&h(".cargo")).is_allow());
        assert!(!vet(Path::new("/tmp/anything")).is_allow());
    }

    #[test]
    fn allows_real_caches() {
        assert!(vet(&h("Library/Caches/go-build")).is_allow());
        assert!(vet(&h(".cargo/registry/cache")).is_allow());
        assert!(vet(&h(".claude/statsig")).is_allow());
    }

    #[test]
    fn dir_delete_spares_user_folders() {
        assert!(vet_dir(&h("Documents/taxes")).reason().is_some());
        assert!(vet_dir(&h("Desktop/screenshots")).reason().is_some());
        assert!(vet_dir(&h("Library/Caches/go-build")).is_allow());
        // but a typed search may still look inside Downloads
        assert!(vet_search_root(&h("Downloads")).is_allow());
    }
}
