//! The catalog: what counts as junk, where it lives, and how risky it is to remove.
//!
//! Entries are plain data. Nothing here is trusted — every path a finder yields
//! is re-checked by `danger::vet` before it can be deleted.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Cat {
    Agents,
    Dev,
    Mobile,
    Browsers,
    Apps,
    System,
    Downloads,
    Projects,
    Manual,
}

pub const CATS: &[Cat] = &[
    Cat::Agents,
    Cat::Dev,
    Cat::Mobile,
    Cat::Browsers,
    Cat::Apps,
    Cat::System,
    Cat::Downloads,
    Cat::Projects,
    Cat::Manual,
];

impl Cat {
    pub fn title(self) -> &'static str {
        match self {
            Cat::Agents => "AI agents & copilots",
            Cat::Dev => "Build & package caches",
            Cat::Mobile => "Xcode, iOS & Android",
            Cat::Browsers => "Browsers",
            Cat::Apps => "Applications",
            Cat::System => "System junk",
            Cat::Downloads => "Downloads clutter",
            Cat::Projects => "Stale project artifacts",
            Cat::Manual => "Needs Terminal",
        }
    }
    pub fn glyph(self) -> &'static str {
        match self {
            Cat::Agents => "◆",
            Cat::Dev => "⬢",
            Cat::Mobile => "▲",
            Cat::Browsers => "◉",
            Cat::Apps => "▣",
            Cat::System => "⬟",
            Cat::Downloads => "▼",
            Cat::Projects => "◇",
            Cat::Manual => "⚙",
        }
    }
    pub fn blurb(self) -> &'static str {
        match self {
            Cat::Agents => {
                "Coding agents keep transcripts, indexes and model blobs forever. \
                 Credentials are never touched."
            }
            Cat::Dev => {
                "Compilers and package managers cache by design and almost never evict. \
                 This is usually the largest category on a developer's machine."
            }
            Cat::Mobile => {
                "Derived data, device symbols and simulator runtimes. Xcode rebuilds \
                 all of it on demand."
            }
            Cat::Browsers => "Page and GPU caches. Profiles, history and logins are untouched.",
            Cat::Apps => "Electron apps keep large render caches that regenerate silently.",
            Cat::System => "Logs, crash reports, saved window state, thumbnails, Trash.",
            Cat::Downloads => "Installers for apps you already installed, and aborted transfers.",
            Cat::Projects => {
                "Dependency and build folders in repositories you have not touched in a while."
            }
            Cat::Manual => {
                "Real space, but it needs a command this program will not run for you. \
                 Press c to copy the command."
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Risk {
    /// Pure cache. Deleting costs nothing but a cold start.
    Safe,
    /// Rebuilds or re-downloads. Costs time and bandwidth, never data.
    Rebuildable,
    /// Something you might actually want: history, transcripts, archives, models.
    Careful,
}

impl Risk {
    pub fn label(self) -> &'static str {
        match self {
            Risk::Safe => "safe",
            Risk::Rebuildable => "rebuild",
            Risk::Careful => "careful",
        }
    }
}

impl fmt::Display for Risk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    /// Remove the directory itself.
    Dir,
    /// One row for the folder; removes its contents and keeps the folder.
    Clear,
    /// One row per direct child, for folders where granularity matters.
    Each,
    /// Remove exactly this path. Assigned at scan time to anything a typed
    /// finder discovered, where the found path *is* the thing to remove.
    Item,
    /// Report only; `hint` holds the command to run.
    Advise,
}

#[derive(Clone, Debug)]
pub enum Finder {
    /// One literal path (supports a leading `~`).
    At(&'static str),
    /// Entries of `base` whose name matches a `*` pattern.
    Glob {
        base: &'static str,
        pat: &'static str,
    },
    /// Directories named one of `names`, found under any of `roots`, untouched
    /// for at least `older_days`.
    Named {
        roots: &'static [&'static str],
        names: &'static [&'static str],
        depth: usize,
        older_days: u64,
    },
    /// Files under `base` with one of `exts`, at least `min_mb`, at least
    /// `older_days` old.
    Files {
        base: &'static str,
        exts: &'static [&'static str],
        depth: usize,
        min_mb: u64,
        older_days: u64,
    },
    /// Files anywhere under `roots` with an exact name (e.g. `.DS_Store`).
    Litter {
        roots: &'static [&'static str],
        names: &'static [&'static str],
        depth: usize,
    },
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub cat: Cat,
    pub label: &'static str,
    pub detail: &'static str,
    pub risk: Risk,
    pub act: Act,
    pub find: Finder,
    /// Shown for `Act::Advise`, and as a footnote otherwise.
    pub hint: &'static str,
}

fn r(
    cat: Cat,
    label: &'static str,
    risk: Risk,
    act: Act,
    find: Finder,
    detail: &'static str,
) -> Rule {
    Rule {
        cat,
        label,
        detail,
        risk,
        act,
        find,
        hint: "",
    }
}

fn advise(label: &'static str, hint: &'static str, detail: &'static str) -> Rule {
    Rule {
        cat: Cat::Manual,
        label,
        detail,
        risk: Risk::Rebuildable,
        act: Act::Advise,
        find: Finder::At(""),
        hint,
    }
}

/// Roots worth scanning for stale build output. Keep this list conservative —
/// a false positive here costs someone a rebuild.
const CODE_ROOTS: &[&str] = &[
    "~/projects",
    "~/work",
    "~/synheart",
    "~/study",
    "~/odin",
    "~/securo",
    "~/exploree-app",
    "~/terminal-app",
    "~/enochCodes-notes",
    "~/src",
    "~/code",
    "~/dev",
    "~/repos",
    "~/go/src",
];

const LITTER_ROOTS: &[&str] = &[
    "~/Downloads",
    "~/Documents",
    "~/Desktop",
    "~/projects",
    "~/work",
    "~/synheart",
];

pub fn catalog() -> Vec<Rule> {
    use Act::*;
    use Cat::*;
    use Finder::*;
    use Risk::*;

    vec![
        // ───────────────────────────── AI agents & copilots ─────────────────
        r(
            Agents,
            "Claude Code · telemetry cache",
            Safe,
            Dir,
            At("~/.claude/statsig"),
            "Feature-flag and analytics cache. Regenerates on next launch.",
        ),
        r(
            Agents,
            "Claude Code · shell snapshots",
            Safe,
            Clear,
            At("~/.claude/shell-snapshots"),
            "Captured shell environments, one per session. Rewritten every run.",
        ),
        r(
            Agents,
            "Claude Code · todo state",
            Safe,
            Clear,
            At("~/.claude/todos"),
            "Per-session task lists from finished sessions.",
        ),
        r(
            Agents,
            "Claude Code · downloads",
            Safe,
            Clear,
            At("~/.claude/downloads"),
            "Files the agent fetched during past sessions.",
        ),
        r(
            Agents,
            "Claude Code · session transcripts",
            Careful,
            Each,
            At("~/.claude/projects"),
            "Full JSONL conversation history per project. Often the biggest item here — \
           and the one you lose /resume on.",
        ),
        r(
            Agents,
            "Claude Code · node cache",
            Safe,
            Dir,
            At("~/Library/Caches/claude-cli-nodejs"),
            "CLI runtime cache.",
        ),
        r(
            Agents,
            "Claude Code · node cache (XDG)",
            Safe,
            Dir,
            At("~/.cache/claude-cli-nodejs"),
            "CLI runtime cache in the XDG location.",
        ),
        r(
            Agents,
            "Claude Desktop · logs",
            Safe,
            Clear,
            At("~/Library/Logs/Claude"),
            "Desktop app logs.",
        ),
        r(
            Agents,
            "Claude Desktop · cache",
            Safe,
            Clear,
            At("~/Library/Caches/com.anthropic.claudefordesktop"),
            "Electron render and network cache.",
        ),
        r(
            Agents,
            "Codex · logs",
            Safe,
            Clear,
            At("~/.codex/log"),
            "OpenAI Codex CLI logs.",
        ),
        r(
            Agents,
            "Codex · sessions",
            Careful,
            Each,
            At("~/.codex/sessions"),
            "Recorded Codex sessions — your prompt history.",
        ),
        r(
            Agents,
            "Gemini CLI · temp",
            Safe,
            Clear,
            At("~/.gemini/tmp"),
            "Scratch files from Gemini CLI runs.",
        ),
        r(
            Agents,
            "Gemini CLI · cache",
            Safe,
            Dir,
            At("~/.gemini/cache"),
            "Response and model metadata cache.",
        ),
        r(
            Agents,
            "GitHub Copilot · cache",
            Safe,
            Dir,
            At("~/Library/Caches/copilot"),
            "Completion and index cache.",
        ),
        r(
            Agents,
            "GitHub Copilot · CLI state",
            Safe,
            Dir,
            At("~/.copilot/cache"),
            "Copilot CLI cache.",
        ),
        r(
            Agents,
            "Cline · cache",
            Safe,
            Dir,
            At("~/.cline/cache"),
            "Cline agent cache.",
        ),
        r(
            Agents,
            "Antigravity · caches",
            Safe,
            Dir,
            Glob {
                base: "~/.antigravity",
                pat: "*Cache*",
            },
            "Editor and extension caches.",
        ),
        r(
            Agents,
            "Antigravity · logs",
            Safe,
            Dir,
            At("~/.antigravity/logs"),
            "Agent logs.",
        ),
        r(
            Agents,
            "Cursor · render cache",
            Safe,
            Dir,
            Glob {
                base: "~/Library/Application Support/Cursor",
                pat: "*Cache*",
            },
            "Electron caches — Cache, CachedData, Code Cache, GPUCache.",
        ),
        r(
            Agents,
            "Cursor · logs",
            Safe,
            Dir,
            At("~/Library/Application Support/Cursor/logs"),
            "Editor logs.",
        ),
        r(
            Agents,
            "Cursor · workspace storage",
            Careful,
            Each,
            At("~/Library/Application Support/Cursor/User/workspaceStorage"),
            "Per-workspace state: open editors, undo stacks, chat threads.",
        ),
        r(
            Agents,
            "Cursor · local history",
            Careful,
            Each,
            At("~/Library/Application Support/Cursor/User/History"),
            "Cursor's own file-version history. Your only undo for some edits.",
        ),
        r(
            Agents,
            "Windsurf · render cache",
            Safe,
            Dir,
            Glob {
                base: "~/Library/Application Support/Windsurf",
                pat: "*Cache*",
            },
            "Electron caches.",
        ),
        r(
            Agents,
            "Continue · index",
            Rebuildable,
            Dir,
            At("~/.continue/index"),
            "Codebase embedding index. Rebuilds on next open.",
        ),
        r(
            Agents,
            "Continue · sessions",
            Careful,
            Each,
            At("~/.continue/sessions"),
            "Saved Continue chat sessions.",
        ),
        r(
            Agents,
            "Zed · cache",
            Safe,
            Dir,
            At("~/Library/Caches/dev.zed.Zed"),
            "Editor cache.",
        ),
        r(
            Agents,
            "ChatGPT desktop · cache",
            Safe,
            Clear,
            At("~/Library/Caches/com.openai.chat"),
            "Desktop app cache.",
        ),
        r(
            Agents,
            "Ollama · local models",
            Careful,
            Each,
            At("~/.ollama/models"),
            "Downloaded weights. Frequently tens of GB — and a slow re-pull.",
        ),
        r(
            Agents,
            "LM Studio · local models",
            Careful,
            Each,
            At("~/.lmstudio/models"),
            "Downloaded weights.",
        ),
        r(
            Agents,
            "Hugging Face · hub cache",
            Rebuildable,
            Clear,
            At("~/.cache/huggingface"),
            "Downloaded models and datasets. Re-fetched on demand.",
        ),
        r(
            Agents,
            "PyTorch · hub cache",
            Rebuildable,
            Clear,
            At("~/.cache/torch"),
            "Pretrained checkpoints.",
        ),
        r(
            Agents,
            "Whisper · model cache",
            Rebuildable,
            Clear,
            At("~/.cache/whisper"),
            "Downloaded speech models.",
        ),
        // Conventional cache/log subfolders for less common agent CLIs. A rule
        // whose path does not exist yields nothing and never appears in the UI,
        // so covering them costs nothing.
        r(
            Agents,
            "cagent · cache",
            Safe,
            Clear,
            At("~/.cagent/cache"),
            "Agent runtime cache.",
        ),
        r(
            Agents,
            "macle · cache",
            Safe,
            Clear,
            At("~/.macle/cache"),
            "Agent runtime cache.",
        ),
        r(
            Agents,
            "zencode · cache",
            Safe,
            Clear,
            At("~/.zencode/cache"),
            "Agent runtime cache.",
        ),
        r(
            Agents,
            "agents · cache",
            Safe,
            Clear,
            At("~/.agents/cache"),
            "Shared agent cache directory.",
        ),
        r(
            Agents,
            "gateguard · logs",
            Safe,
            Clear,
            At("~/.gateguard/logs"),
            "Agent logs.",
        ),
        r(
            Agents,
            "WakaTime · logs",
            Safe,
            Clear,
            Files {
                base: "~/.wakatime",
                exts: &["log"],
                depth: 1,
                min_mb: 0,
                older_days: 0,
            },
            "Editor-time tracking logs. The CLI binary beside them is left alone.",
        ),
        // ───────────────────────────── Build & package caches ───────────────
        r(
            Dev,
            "Go · build cache",
            Safe,
            Clear,
            At("~/Library/Caches/go-build"),
            "Compiled package objects. Uncapped and never evicted — routinely the single \
           largest folder on a Go developer's machine.",
        ),
        r(
            Dev,
            "Go · module cache",
            Rebuildable,
            Dir,
            At("~/go/pkg/mod"),
            "Downloaded module sources. Files are read-only; removal fixes permissions first.",
        ),
        r(
            Dev,
            "Go · sum database",
            Safe,
            Dir,
            At("~/go/pkg/sumdb"),
            "Checksum database mirror.",
        ),
        r(
            Dev,
            "gopls · index",
            Safe,
            Clear,
            At("~/Library/Caches/gopls"),
            "Language-server index, one entry per module you have opened.",
        ),
        r(
            Dev,
            "goimports · cache",
            Safe,
            Clear,
            At("~/Library/Caches/goimports"),
            "Import resolution cache.",
        ),
        r(
            Dev,
            "golangci-lint · cache",
            Safe,
            Clear,
            At("~/Library/Caches/golangci-lint"),
            "Linter result cache.",
        ),
        r(
            Dev,
            "staticcheck · cache",
            Safe,
            Clear,
            At("~/Library/Caches/staticcheck"),
            "Analysis cache.",
        ),
        r(
            Dev,
            "Cargo · registry cache",
            Rebuildable,
            Dir,
            At("~/.cargo/registry/cache"),
            "Downloaded crate archives.",
        ),
        r(
            Dev,
            "Cargo · registry sources",
            Rebuildable,
            Dir,
            At("~/.cargo/registry/src"),
            "Unpacked crate sources.",
        ),
        r(
            Dev,
            "Cargo · registry index",
            Rebuildable,
            Dir,
            At("~/.cargo/registry/index"),
            "Crates.io index clone.",
        ),
        r(
            Dev,
            "Cargo · git checkouts",
            Rebuildable,
            Dir,
            At("~/.cargo/git/checkouts"),
            "Git dependency working copies.",
        ),
        r(
            Dev,
            "Cargo · git database",
            Rebuildable,
            Dir,
            At("~/.cargo/git/db"),
            "Bare clones of git dependencies.",
        ),
        r(
            Dev,
            "Gradle · caches",
            Rebuildable,
            Dir,
            At("~/.gradle/caches"),
            "Resolved dependencies and build output. Re-downloads on next build.",
        ),
        r(
            Dev,
            "Gradle · daemon logs",
            Safe,
            Clear,
            At("~/.gradle/daemon"),
            "Background daemon logs.",
        ),
        r(
            Dev,
            "Gradle · native",
            Safe,
            Dir,
            At("~/.gradle/native"),
            "Extracted native helpers.",
        ),
        r(
            Dev,
            "npm · package cache",
            Rebuildable,
            Dir,
            At("~/.npm/_cacache"),
            "Tarball and metadata cache.",
        ),
        r(
            Dev,
            "npm · npx cache",
            Rebuildable,
            Dir,
            At("~/.npm/_npx"),
            "Throwaway installs from npx invocations.",
        ),
        r(
            Dev,
            "npm · logs",
            Safe,
            Clear,
            At("~/.npm/_logs"),
            "Install failure logs.",
        ),
        r(
            Dev,
            "pnpm · content store",
            Rebuildable,
            Clear,
            At("~/Library/pnpm/store"),
            "Global content-addressed package store.",
        ),
        r(
            Dev,
            "Yarn · cache",
            Rebuildable,
            Clear,
            At("~/Library/Caches/Yarn"),
            "Package cache.",
        ),
        r(
            Dev,
            "Bun · install cache",
            Rebuildable,
            Clear,
            At("~/.bun/install/cache"),
            "Package cache.",
        ),
        r(
            Dev,
            "Deno · cache",
            Rebuildable,
            Clear,
            At("~/Library/Caches/deno"),
            "Remote module cache.",
        ),
        r(
            Dev,
            "Maven · repository",
            Rebuildable,
            Clear,
            At("~/.m2/repository"),
            "Downloaded artifacts.",
        ),
        r(
            Dev,
            "Dart/Flutter · package cache",
            Rebuildable,
            Clear,
            At("~/.pub-cache/hosted"),
            "Downloaded packages. Every project needs `flutter pub get` afterwards, \
           and you will need network to build.",
        ),
        r(
            Dev,
            "pip · wheel cache",
            Rebuildable,
            Clear,
            At("~/Library/Caches/pip"),
            "Downloaded and built wheels.",
        ),
        r(
            Dev,
            "pip · wheel cache (XDG)",
            Rebuildable,
            Clear,
            At("~/.cache/pip"),
            "Downloaded and built wheels.",
        ),
        r(
            Dev,
            "uv · cache",
            Rebuildable,
            Clear,
            At("~/.cache/uv"),
            "Python package cache.",
        ),
        r(
            Dev,
            "Composer · cache",
            Rebuildable,
            Clear,
            At("~/Library/Caches/composer"),
            "PHP package cache.",
        ),
        r(
            Dev,
            "Composer · cache (home)",
            Rebuildable,
            Clear,
            At("~/.composer/cache"),
            "PHP package cache.",
        ),
        r(
            Dev,
            "Puppeteer · browser builds",
            Rebuildable,
            Clear,
            At("~/.cache/puppeteer"),
            "Downloaded Chromium builds, one per version pinned by a project.",
        ),
        r(
            Dev,
            "Playwright · browser builds",
            Rebuildable,
            Clear,
            At("~/Library/Caches/ms-playwright"),
            "Downloaded browser binaries.",
        ),
        r(
            Dev,
            "Playwright for Go · browsers",
            Rebuildable,
            Clear,
            At("~/Library/Caches/ms-playwright-go"),
            "Downloaded browser binaries.",
        ),
        r(
            Dev,
            "Prisma · engines",
            Rebuildable,
            Clear,
            At("~/.cache/prisma"),
            "Query engine binaries.",
        ),
        r(
            Dev,
            "Zig · cache",
            Safe,
            Clear,
            At("~/.cache/zig"),
            "Compilation cache.",
        ),
        r(
            Dev,
            "SwiftPM · cache",
            Rebuildable,
            Clear,
            At("~/Library/Caches/org.swift.swiftpm"),
            "Package cache.",
        ),
        r(
            Dev,
            "SwiftPM · cache (home)",
            Rebuildable,
            Clear,
            At("~/.swiftpm/cache"),
            "Package cache.",
        ),
        r(
            Dev,
            "RubyGems · cache",
            Rebuildable,
            Clear,
            At("~/.gem/cache"),
            "Downloaded gems.",
        ),
        r(
            Dev,
            "Bundler · cache",
            Rebuildable,
            Clear,
            At("~/.bundle/cache"),
            "Downloaded gems.",
        ),
        r(
            Dev,
            "Hex · packages",
            Rebuildable,
            Clear,
            At("~/.hex/packages"),
            "Elixir package cache.",
        ),
        r(
            Dev,
            "Mix · build cache",
            Rebuildable,
            Clear,
            At("~/Library/Caches/mix"),
            "Elixir build cache.",
        ),
        r(
            Dev,
            "NuGet · packages",
            Rebuildable,
            Clear,
            At("~/.nuget/packages"),
            ".NET package cache.",
        ),
        r(
            Dev,
            "node-gyp · headers",
            Rebuildable,
            Clear,
            At("~/Library/Caches/node-gyp"),
            "Node headers for native builds.",
        ),
        r(
            Dev,
            "Electron · binaries",
            Rebuildable,
            Clear,
            At("~/Library/Caches/electron"),
            "Downloaded Electron runtimes.",
        ),
        r(
            Dev,
            "electron-builder · cache",
            Rebuildable,
            Clear,
            At("~/Library/Caches/electron-builder"),
            "Packaging toolchain cache.",
        ),
        r(
            Dev,
            "Next.js · SWC binaries",
            Safe,
            Clear,
            At("~/Library/Caches/next-swc"),
            "Compiler binaries, one per version.",
        ),
        r(
            Dev,
            "Turborepo · cache",
            Safe,
            Clear,
            At("~/Library/Caches/turbo"),
            "Task result cache.",
        ),
        r(
            Dev,
            "TypeScript · server cache",
            Safe,
            Clear,
            At("~/Library/Caches/typescript"),
            "Type acquisition cache.",
        ),
        r(
            Dev,
            "Eclipse JDT · workspaces",
            Safe,
            Dir,
            Glob {
                base: "~/Library/Caches",
                pat: "jdtls-*",
            },
            "Java language-server workspace per project. One folder per repo, \
           none of them shared.",
        ),
        r(
            Dev,
            "C/C++ IntelliSense · cache",
            Safe,
            Clear,
            At("~/Library/Caches/vscode-cpptools"),
            "Browse database for C/C++ projects.",
        ),
        r(
            Dev,
            "Bazel · cache",
            Rebuildable,
            Clear,
            At("~/Library/Caches/bazel"),
            "Build output base.",
        ),
        r(
            Dev,
            "SonarLint · storage",
            Safe,
            Clear,
            At("~/.sonarlint"),
            "Rule and analyzer storage.",
        ),
        r(
            Dev,
            "Sonar · cache",
            Safe,
            Clear,
            At("~/.sonar/cache"),
            "Analyzer downloads.",
        ),
        r(
            Dev,
            "Dart · analysis server",
            Safe,
            Clear,
            At("~/.dartServer"),
            "Analyzer state and logs.",
        ),
        r(
            Dev,
            "Flutter DevTools · cache",
            Safe,
            Clear,
            At("~/.flutter-devtools"),
            "DevTools assets.",
        ),
        r(
            Dev,
            "Biome · cache",
            Safe,
            Clear,
            At("~/.biome"),
            "Formatter and linter cache.",
        ),
        r(
            Dev,
            "Matplotlib · font cache",
            Safe,
            Clear,
            At("~/.matplotlib"),
            "Font index. Rebuilds on first plot.",
        ),
        // ───────────────────────────── Xcode, iOS & Android ─────────────────
        r(
            Mobile,
            "Xcode · derived data",
            Safe,
            Clear,
            At("~/Library/Developer/Xcode/DerivedData"),
            "Build intermediates and indexes. The first thing to delete, always.",
        ),
        r(
            Mobile,
            "Xcode · iOS device symbols",
            Rebuildable,
            Each,
            At("~/Library/Developer/Xcode/iOS DeviceSupport"),
            "Symbol sets per iOS version, ~3 GB each. Re-downloads when you next \
           attach that device.",
        ),
        r(
            Mobile,
            "Xcode · watchOS device symbols",
            Rebuildable,
            Each,
            At("~/Library/Developer/Xcode/watchOS DeviceSupport"),
            "Symbol sets per watchOS version.",
        ),
        r(
            Mobile,
            "Xcode · tvOS device symbols",
            Rebuildable,
            Each,
            At("~/Library/Developer/Xcode/tvOS DeviceSupport"),
            "Symbol sets per tvOS version.",
        ),
        r(
            Mobile,
            "Xcode · archives",
            Careful,
            Each,
            At("~/Library/Developer/Xcode/Archives"),
            "Shipped build archives. You need these to re-sign or symbolicate a \
           release you already submitted.",
        ),
        r(
            Mobile,
            "Xcode · device logs",
            Safe,
            Clear,
            At("~/Library/Developer/Xcode/iOS Device Logs"),
            "Crash logs pulled from attached devices.",
        ),
        r(
            Mobile,
            "Xcode · Interface Builder cache",
            Safe,
            Clear,
            At("~/Library/Developer/Xcode/UserData/IB Support"),
            "Storyboard render cache.",
        ),
        r(
            Mobile,
            "Xcode · coding assistant models",
            Rebuildable,
            Clear,
            At("~/Library/Developer/Xcode/CodingAssistant"),
            "On-device completion models. Re-downloaded by Xcode.",
        ),
        r(
            Mobile,
            "Xcode · application cache",
            Safe,
            Clear,
            At("~/Library/Caches/com.apple.dt.Xcode"),
            "General Xcode cache.",
        ),
        r(
            Mobile,
            "Simulator · caches",
            Safe,
            Clear,
            At("~/Library/Developer/CoreSimulator/Caches"),
            "Simulator support caches.",
        ),
        r(
            Mobile,
            "Simulator · devices",
            Careful,
            Each,
            At("~/Library/Developer/CoreSimulator/Devices"),
            "Every created simulator plus its installed apps and data. \
           Prefer `xcrun simctl delete unavailable` — see Needs Terminal.",
        ),
        r(
            Mobile,
            "XCTest · devices",
            Safe,
            Clear,
            At("~/Library/Developer/XCTestDevices"),
            "Test runner devices.",
        ),
        r(
            Mobile,
            "Android · emulator images",
            Careful,
            Each,
            At("~/Library/Android/sdk/system-images"),
            "Emulator system images, 2–8 GB each.",
        ),
        r(
            Mobile,
            "Android · virtual devices",
            Careful,
            Each,
            At("~/.android/avd"),
            "Created emulators and their disk images.",
        ),
        r(
            Mobile,
            "Android · cache",
            Safe,
            Clear,
            At("~/.android/cache"),
            "SDK manager cache.",
        ),
        r(
            Mobile,
            "Android Studio · caches",
            Safe,
            Dir,
            Glob {
                base: "~/Library/Caches/Google",
                pat: "AndroidStudio*",
            },
            "IDE index and build caches, one folder per installed version.",
        ),
        r(
            Mobile,
            "Android Studio · logs",
            Safe,
            Dir,
            Glob {
                base: "~/Library/Logs/Google",
                pat: "AndroidStudio*",
            },
            "IDE logs.",
        ),
        r(
            Mobile,
            "JetBrains · caches",
            Safe,
            Clear,
            At("~/Library/Caches/JetBrains"),
            "Index and local history for IntelliJ-family IDEs.",
        ),
        // ───────────────────────────── Browsers ─────────────────────────────
        r(
            Browsers,
            "Brave · cache",
            Safe,
            Clear,
            At("~/Library/Caches/BraveSoftware"),
            "Page, code and GPU cache. Logins and history live elsewhere.",
        ),
        r(
            Browsers,
            "Brave · app cache",
            Safe,
            Clear,
            At("~/Library/Caches/com.brave.Browser"),
            "Application-level cache.",
        ),
        r(
            Browsers,
            "Chrome · cache",
            Safe,
            Clear,
            At("~/Library/Caches/Google/Chrome"),
            "Page and GPU cache.",
        ),
        r(
            Browsers,
            "Chrome · app cache",
            Safe,
            Clear,
            At("~/Library/Caches/com.google.Chrome"),
            "Application-level cache.",
        ),
        r(
            Browsers,
            "Safari · cache",
            Safe,
            Clear,
            At("~/Library/Caches/com.apple.Safari"),
            "Page cache.",
        ),
        r(
            Browsers,
            "Firefox · cache",
            Safe,
            Clear,
            At("~/Library/Caches/Firefox"),
            "Page cache across profiles.",
        ),
        r(
            Browsers,
            "Arc · cache",
            Safe,
            Clear,
            At("~/Library/Caches/company.thebrowser.Browser"),
            "Page and GPU cache.",
        ),
        r(
            Browsers,
            "Edge · cache",
            Safe,
            Clear,
            At("~/Library/Caches/com.microsoft.edgemac"),
            "Page and GPU cache.",
        ),
        // ───────────────────────────── Applications ─────────────────────────
        r(
            Apps,
            "VS Code · render cache",
            Safe,
            Dir,
            Glob {
                base: "~/Library/Application Support/Code",
                pat: "*Cache*",
            },
            "Cache, CachedData, Code Cache, GPUCache, CachedExtensionVSIXs.",
        ),
        r(
            Apps,
            "VS Code · logs",
            Safe,
            Clear,
            At("~/Library/Application Support/Code/logs"),
            "Window and extension host logs.",
        ),
        r(
            Apps,
            "VS Code · workspace storage",
            Careful,
            Each,
            At("~/Library/Application Support/Code/User/workspaceStorage"),
            "Per-workspace state. Clearing resets open editors and extension state.",
        ),
        r(
            Apps,
            "VS Code · local history",
            Careful,
            Each,
            At("~/Library/Application Support/Code/User/History"),
            "Editor's own file-version history.",
        ),
        r(
            Apps,
            "Slack · caches",
            Safe,
            Dir,
            Glob {
                base: "~/Library/Application Support/Slack",
                pat: "*Cache*",
            },
            "Render and service-worker caches.",
        ),
        r(
            Apps,
            "Slack · service worker",
            Safe,
            Clear,
            At("~/Library/Application Support/Slack/Service Worker/CacheStorage"),
            "Cached assets for the web client.",
        ),
        r(
            Apps,
            "Discord · caches",
            Safe,
            Dir,
            Glob {
                base: "~/Library/Application Support/discord",
                pat: "*Cache*",
            },
            "Render caches.",
        ),
        r(
            Apps,
            "Spotify · offline cache",
            Rebuildable,
            Clear,
            At("~/Library/Caches/com.spotify.client"),
            "Cached audio. Re-streams on demand.",
        ),
        r(
            Apps,
            "Raycast · cache",
            Safe,
            Clear,
            At("~/Library/Caches/com.raycast.macos"),
            "Index and extension cache.",
        ),
        r(
            Apps,
            "Postman · cache",
            Safe,
            Clear,
            At("~/Library/Application Support/Postman/Cache"),
            "Response and asset cache.",
        ),
        r(
            Apps,
            "Docker Desktop · logs",
            Safe,
            Clear,
            At("~/Library/Containers/com.docker.docker/Data/log"),
            "Daemon and VM logs. The disk image itself needs `docker system prune`.",
        ),
        r(
            Apps,
            "Figma · cache",
            Safe,
            Clear,
            At("~/Library/Caches/com.figma.Desktop"),
            "Document and font cache.",
        ),
        r(
            Apps,
            "Notion · cache",
            Safe,
            Dir,
            Glob {
                base: "~/Library/Application Support/Notion",
                pat: "*Cache*",
            },
            "Render caches.",
        ),
        // ───────────────────────────── System junk ──────────────────────────
        r(
            System,
            "Trash",
            Rebuildable,
            Clear,
            At("~/.Trash"),
            "Already-deleted files, still occupying space.",
        ),
        r(
            System,
            "User logs",
            Safe,
            Clear,
            At("~/Library/Logs"),
            "Application logs.",
        ),
        r(
            System,
            "Crash reports",
            Safe,
            Clear,
            At("~/Library/Application Support/CrashReporter"),
            "Crash and hang reports.",
        ),
        r(
            System,
            "Saved application state",
            Safe,
            Clear,
            At("~/Library/Saved Application State"),
            "Restored window positions per app. Regenerates; apps reopen blank once.",
        ),
        r(
            System,
            "QuickLook thumbnails",
            Safe,
            Clear,
            At("~/Library/Caches/com.apple.QuickLook.thumbnailcache"),
            "Finder preview thumbnails.",
        ),
        r(
            System,
            "Help viewer cache",
            Safe,
            Clear,
            At("~/Library/Caches/com.apple.helpd"),
            "Downloaded help content.",
        ),
        r(
            System,
            "Mail downloads",
            Rebuildable,
            Clear,
            At("~/Library/Containers/com.apple.mail/Data/Library/Mail Downloads"),
            "Attachments opened from Mail. Still on the server.",
        ),
        r(
            System,
            ".DS_Store litter",
            Safe,
            Clear,
            Litter {
                roots: LITTER_ROOTS,
                names: &[".DS_Store"],
                depth: 8,
            },
            "Finder metadata files scattered through your folders.",
        ),
        // ───────────────────────────── Downloads clutter ────────────────────
        r(
            Downloads,
            "App installers",
            Rebuildable,
            Clear,
            Files {
                base: "~/Downloads",
                exts: &["dmg", "pkg"],
                depth: 2,
                min_mb: 20,
                older_days: 7,
            },
            "Disk images and installer packages over 20 MB, older than a week. \
           The apps are installed; these are the wrappers.",
        ),
        r(
            Downloads,
            "Disk images & archives",
            Rebuildable,
            Clear,
            Files {
                base: "~/Downloads",
                exts: &["iso", "img"],
                depth: 2,
                min_mb: 100,
                older_days: 30,
            },
            "Large images older than a month.",
        ),
        r(
            Downloads,
            "Aborted downloads",
            Safe,
            Clear,
            Files {
                base: "~/Downloads",
                exts: &["crdownload", "download", "part", "partial"],
                depth: 2,
                min_mb: 1,
                older_days: 1,
            },
            "Incomplete transfers. Fragments, not files.",
        ),
        // ───────────────────────────── Stale project artifacts ──────────────
        r(
            Projects,
            "node_modules (90+ days idle)",
            Rebuildable,
            Clear,
            Named {
                roots: CODE_ROOTS,
                names: &["node_modules"],
                depth: 4,
                older_days: 90,
            },
            "Dependency trees in repositories untouched for three months. \
           `npm install` restores them.",
        ),
        r(
            Projects,
            "Rust target dirs (90+ days idle)",
            Rebuildable,
            Clear,
            Named {
                roots: CODE_ROOTS,
                names: &["target"],
                depth: 4,
                older_days: 90,
            },
            "Rust build output. Often the largest folder in a repo.",
        ),
        r(
            Projects,
            "Build output (90+ days idle)",
            Rebuildable,
            Clear,
            Named {
                roots: CODE_ROOTS,
                names: &[
                    ".next",
                    "dist",
                    "build",
                    ".turbo",
                    ".parcel-cache",
                    ".dart_tool",
                ],
                depth: 4,
                older_days: 90,
            },
            "Framework build directories in idle repositories.",
        ),
        r(
            Projects,
            "CocoaPods (90+ days idle)",
            Rebuildable,
            Clear,
            Named {
                roots: CODE_ROOTS,
                names: &["Pods"],
                depth: 4,
                older_days: 90,
            },
            "Installed pods. `pod install` restores them.",
        ),
        r(
            Projects,
            "Python caches",
            Safe,
            Clear,
            Named {
                roots: CODE_ROOTS,
                names: &["__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache"],
                depth: 5,
                older_days: 0,
            },
            "Bytecode and tool caches. Always regenerated.",
        ),
        // ───────────────────────────── Needs Terminal ───────────────────────
        advise(
            "Time Machine local snapshots",
            "sudo tmutil thinlocalsnapshots / 999999999999 4",
            "macOS keeps hourly local backups on the internal SSD. They are the single \
             largest hidden consumer on most Macs and appear only as 'System Data'. \
             Check first with: tmutil listlocalsnapshots /",
        ),
        advise(
            "Docker / OrbStack disk image",
            "docker system prune -a --volumes && orb shrink",
            "Images and volumes live inside one sparse disk file. Deleting the file \
             destroys everything; pruning from inside is what actually reclaims space.",
        ),
        advise(
            "Unavailable iOS simulators",
            "xcrun simctl delete unavailable",
            "Removes simulators whose runtime is no longer installed. Safe, and \
             usually worth several GB.",
        ),
        advise(
            "Unused simulator runtimes",
            "xcrun simctl runtime list && xcrun simctl runtime delete <id>",
            "Each installed iOS runtime is 5–12 GB. Keep the versions you actually test on.",
        ),
        advise(
            "Homebrew cache",
            "brew cleanup -s --prune=all && rm -rf \"$(brew --cache)\"",
            "Old formula versions and every downloaded bottle.",
        ),
        advise(
            "Go caches, the supported way",
            "go clean -cache -modcache -fuzzcache",
            "Equivalent to the Go entries above, run through the toolchain so it \
             rebuilds its bookkeeping correctly.",
        ),
        advise(
            "Unified system log store",
            "sudo log erase --all",
            "The system-wide log database. Frequently 2–5 GB.",
        ),
        advise(
            "Old iOS device backups",
            "open ~/Library/Application\\ Support/MobileSync/Backup",
            "Each archived iPhone backup is 10–60 GB. Review before deleting — \
             these are not regenerated.",
        ),
    ]
}
