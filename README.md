# dustoff

**Deep storage cleaner for macOS with a terminal UI.**

Most disk cleaners for developers stop at `node_modules` and `target`. Those are
rarely where the space went. On a working Mac the real weight sits in places you
never open: a Go build cache that is never evicted, four Java language-server
workspaces, a browser cache measured in gigabytes, iOS device symbol sets, and —
increasingly — whatever your coding agents have been writing down all year.

dustoff knows **149 of those places**, plus **8 more** that only a Terminal
command can reach. It measures them in parallel, rates each one by what you
actually lose, and deletes nothing until you say so.

```
  dustoff   31.7 G reclaimable  ·  64 items  ·  9.97 G selected in 2 items
  disk  412 G used of 994 G  ·  582 G free   mode: trash
  ████████████████████████████████████░░░░░░░░░░░░░░  71% full
────────────────────────────────────────────────────────────────────────────
╭ categories ────────────────╮╭ Build & package caches  ·  8.38 G ─────────╮
│  ◆ AI agents & cop…  19.9 G││  ◉ ██████████   6.60 G safe    Go · build  │
│ ▌⬢ Build & package…  8.38 G││  ○ ▍·········    220 M safe    Eclipse JDT │
│  ◉ Browsers          3.36 G││  ○ ▎·········    187 M safe    Eclipse JDT │
│  ⬟ System junk        732 K││  ○ ██▏·······   1.38 G rebuild Cargo · reg │
│  ⚙ Needs Terminal         —││                                            │
╰────────────────────────────╯╰────────────────────────────────────────────╯
╭ detail ──────────────────────────────────────────────────────────────────╮
│  ~/Library/Caches/go-build   48,211 files                                │
│  Compiled package objects. Uncapped and never evicted — routinely the     │
│  single largest folder on a Go developer's machine.                      │
╰──────────────────────────────────────────────────────────────────────────╯
 space select  a category  A recommended  d clean  t mode  / find  ? help
```

## Why another cleaner

[kondo](https://github.com/tbillington/kondo) is excellent at what it does, and
`cruft` and `chaff` cover similar ground. All three are **project** cleaners:
point them at a directory, they reclaim build artifacts.

dustoff is a **machine** cleaner. Stale project directories are one of its nine
categories, not the whole product. The other eight are the ones that quietly add
up, and the AI-agent category is not covered by any of them.

## Install

Needs Rust 1.85 or newer. If you don't have it:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then:

```sh
git clone https://github.com/enochCodes/dustoff.git
cd dustoff
cargo build --release
cp target/release/dustoff /usr/local/bin/
```

## Use

```sh
dustoff                          # interactive. Dry run until you press t.
dustoff --scan                   # ranked report, no interface, no deletion
dustoff --scan --verbose         # ...with every individual path
dustoff --clean                  # show the recommended set; still a dry run
dustoff --clean --trash          # move the recommended set to the Trash (asks first)
dustoff --clean --permanent -y   # no prompt. For a cron job you trust.
```

`--clean` only ever selects `safe` and `rebuild` items. A scripted run can never
touch anything marked `careful`, by construction.

### Keys

| key | |
|---|---|
| `↑ ↓` / `j k` | move |
| `← →` / `h l` / `tab` | switch pane |
| `space` | select the highlighted item |
| `a` | select everything in this category |
| `A` | select all safe + rebuild items, everywhere |
| `n` | clear the selection |
| `d` | clean the selection (asks first) |
| `t` | cycle dry run → trash → permanent |
| `c` | copy a Needs-Terminal command to the clipboard |
| `/` | filter by name or path |
| `r` | rescan |
| `g` / `G` | top / bottom |
| `q` | quit |

## Risk levels

Every entry carries one, and rows are coloured by it.

| | |
|---|---|
| **safe** | Pure cache. Costs a cold start and nothing else. |
| **rebuild** | Re-downloads or recompiles. Costs time and bandwidth, never data. |
| **careful** | Session transcripts, Xcode archives, simulator devices, local models. Things you might actually want. Never auto-selected. |
| **⊘** | Refused by the safety layer. Shown so you know it was considered, never deletable. |

## What it covers

**AI agents & copilots** — Claude Code (telemetry, shell snapshots, todos, node
cache, and separately its session transcripts), Claude Desktop, Codex, Gemini
CLI, Copilot, Cline, Cursor, Windsurf, Antigravity, Continue, Zed, ChatGPT
desktop, Ollama and LM Studio model stores, Hugging Face / PyTorch / Whisper
caches.

**Build & package caches** — Go build and module caches, gopls, goimports,
golangci-lint, Cargo, Gradle, npm, pnpm, Yarn, Bun, Deno, Maven, pub, pip, uv,
Composer, Puppeteer, Playwright, Prisma, Zig, SwiftPM, RubyGems, Hex, NuGet,
node-gyp, Electron, Next.js SWC, Turborepo, TypeScript, Eclipse JDT workspaces,
C/C++ IntelliSense, Bazel, SonarLint.

**Xcode, iOS & Android** — DerivedData, device symbol sets, archives, simulator
caches and devices, Android emulator images and AVDs, Android Studio, JetBrains.

**Browsers** — Brave, Chrome, Safari, Firefox, Arc, Edge. Caches only; profiles,
history and logins are never in scope.

**Applications** — VS Code, Slack, Discord, Spotify, Raycast, Postman, Figma,
Notion, Docker Desktop logs.

**System junk** — Trash, logs, crash reports, saved application state, QuickLook
thumbnails, Mail downloads, `.DS_Store` litter.

**Downloads clutter** — installers over 20 MB older than a week, large disk
images older than a month, aborted transfers. Age and size thresholds mean this
morning's download is never in the list.

**Stale project artifacts** — `node_modules`, Rust `target`, `.next`, `dist`,
`build`, `.turbo`, `Pods`, `.dart_tool` in repositories untouched for 90+ days,
plus Python caches at any age.

**Needs Terminal** — real space that a cleaner should not take on itself: Time
Machine local snapshots, the Docker/OrbStack disk image, simulator runtimes,
Homebrew's cache, the unified log store, old iOS device backups. These show the
exact command; `c` copies it.

## Safety

One module, [`src/danger.rs`](src/danger.rs), decides whether any path may be
touched, and every destructive path in the program goes through it. The rule
catalog is treated as data that might be wrong.

- Paths must be absolute, must contain no `..`, and must live under `$HOME` or
  `/Volumes`.
- A whole-directory delete must be at least two levels below `$HOME`, so a
  malformed rule cannot resolve to `~/Library` or `~/.cargo`.
- Whole-directory deletes additionally refuse anything inside `Documents`,
  `Desktop`, `Pictures`, `Movies`, `Music`, `Public` or `Applications`. Typed
  finders may still look inside those for an exact filename — that is how
  `.DS_Store` gets cleaned without putting documents at risk.
- A hard deny-list covers `.ssh`, `.gnupg`, `.aws`, `.kube`, keychains, any path
  containing `credentials`, `.npmrc`, `.git-credentials`, iCloud Drive, private
  keys and certificates. Claude Code's `.credentials.json` and `~/.mcp-auth` are
  explicitly out of reach.
- Symlinks are never followed, when sizing or when deleting.
- Every path is vetted twice: once when found, again immediately before removal,
  because a list built minutes ago may be stale.
- Failures are reported, never swallowed.

Default mode is a dry run. `t` cycles to Trash, which is recoverable; a second
press reaches permanent deletion, and the confirmation says so plainly and lists
any `careful` items in the selection.

## Tests

```sh
cargo test
```

17 tests covering the safety rails (traversal, secrets, containment, shallow
paths), the scanner (sizing, age filters, missing paths), deletion semantics
(dry run touches nothing, clear-contents keeps the folder, read-only Go module
trees are removable, paths outside `$HOME` are refused even when passed in
directly), and the interface (renders at 20×8 through 300×90 without panicking,
every overlay, selection rules, filtering).

## Contributing

The catalog in [`src/rules.rs`](src/rules.rs) is plain data — adding a tool is a
few lines, and new entries are very welcome. A rule needs a category, an honest
risk level, and a one-sentence explanation of what is lost. Please don't widen
`danger.rs` to make a rule fit; if the safety layer refuses your path, the rule
is probably too broad.

`cargo fmt` and `cargo clippy --all-targets` should both be clean.

## Notes

macOS reports purgeable space lazily. After a large clean, About This Mac can
lag the real number by several minutes; `df -h /` is immediate.

Linux mostly works — the XDG cache paths resolve and the safety layer is
platform-independent — but the catalog is written for macOS and the `~/Library`
entries simply find nothing elsewhere.

## License

MIT. See [LICENSE](LICENSE).
