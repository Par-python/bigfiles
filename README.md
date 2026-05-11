# bigfiles

[![CI](https://github.com/Par-python/bigfiles/actions/workflows/ci.yml/badge.svg)](https://github.com/Par-python/bigfiles/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/bigfiles.svg)](https://crates.io/crates/bigfiles)
[![Downloads](https://img.shields.io/crates/d/bigfiles.svg)](https://crates.io/crates/bigfiles)
[![License: AGPL v3](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](LICENSE)

A small Rust CLI that walks a directory in parallel, groups files by type, flags stale ones, finds duplicates (hardlink-aware), and renders a color-coded summary in the terminal. Cross-platform: Linux, macOS, Windows.

## What it does

- **Interactive TUI** (`bigfiles tui`) — ncdu-style directory browser with arrow-key navigation
- Walks a directory tree **in parallel** and collects file sizes, extensions, and modified timestamps
- Respects `.gitignore` and `.ignore` files by default (use `--no-ignore` to disable)
- Skips symlinks (no double-counting, no follow-link footguns)
- Groups files into categories: video, images, archives, audio, documents, code, junk, other
- Flags files not modified in the last N years as stale
- Renders a color-coded table with size bars, optionally with the largest files per category
- Finds duplicate files by content hash with **parallel BLAKE3 hashing** and **hardlink awareness** (collapses same-inode files so reclaimable space is honest)
- Interactively deletes stale files **or** duplicate copies with explicit confirmation
- Emits JSON for piping into other tools
- Colorized `--help` output via clap styles

## Install

### Homebrew (macOS / Linux)

```bash
brew install Par-python/bigfiles/bigfiles
```

That command auto-adds the tap on first run. To upgrade later:

```bash
brew upgrade bigfiles
```

To remove:

```bash
brew uninstall bigfiles
brew untap Par-python/bigfiles
```

### crates.io (requires Rust via [rustup](https://rustup.rs))

```bash
cargo install bigfiles
```

To upgrade:

```bash
cargo install bigfiles --force
```

### Pre-built binaries

Download from the [releases page](https://github.com/Par-python/bigfiles/releases) for Linux (x86_64, aarch64), macOS (Intel, Apple Silicon), and Windows (x86_64). Extract and move `bigfiles` (or `bigfiles.exe`) onto your `$PATH`.

**From source:**

```bash
git clone https://github.com/Par-python/bigfiles
cd bigfiles
cargo install --path .
```

## Usage

```bash
# Scan current directory
bigfiles

# Scan a specific path
bigfiles ~/Downloads

# Skip hidden files and dirs, only descend 3 levels
bigfiles ~ --skip-hidden --depth 3

# Show the 5 largest files per category alongside the summary
bigfiles ~/Downloads --top 5

# Exclude paths via glob (repeatable)
bigfiles ~ --exclude 'node_modules' --exclude '*.log' --exclude 'target'

# Don't respect .gitignore / .ignore
bigfiles ~/some-project --no-ignore

# Treat anything not modified in 5+ years as stale (default: 2)
bigfiles ~/Documents --stale-years 5

# Pipe JSON into jq (envelope: { version, root, total_size, skipped, categories })
bigfiles ~/Movies --json | jq '.categories[] | select(.stale_size > 1000000000)'
```

### .gitignore awareness

By default bigfiles uses [the same `ignore` crate that ripgrep uses](https://crates.io/crates/ignore), so `.gitignore`, `.ignore`, and global git excludes are respected automatically. Scanning a Rust project? `target/` is skipped. Node project? `node_modules` is skipped. No flag needed.

Use `--no-ignore` to walk everything regardless.

### Interactive TUI

`bigfiles tui <path>` opens a full-screen ncdu-style directory browser. Sizes are aggregated per directory; the largest entries float to the top.

```bash
bigfiles tui ~
```

Keys: `↑/↓` (or `j/k`) move • `Enter`/`→` descend into directory • `←`/`Backspace` go up • `q`/`Esc` quit • `?` toggle help.

### Find duplicate files

`bigfiles dupes` finds files with identical content. It uses a fast three-stage check, parallelized with `rayon`:

1. Group by size
2. Hash first/last 4 KB (`partial_hash`)
3. Full BLAKE3 hash on remaining candidates

Hardlinks are collapsed by inode before hashing, so multiple paths pointing to the same on-disk file are reported as a single entry (and don't inflate "reclaimable" numbers). When a duplicate group includes hardlinks, the additional paths are shown indented under the primary path.

```bash
# Find dupes >= 1 MB in Downloads
bigfiles dupes ~/Downloads --min-size 1048576

# Default min-size is 1 KB; tune as needed
bigfiles dupes ~/Documents --min-size 1
```

#### Delete duplicate copies (interactive)

`bigfiles dupes --delete` walks each duplicate group and lets you pick which copy to **keep**; the rest are queued for deletion. After all groups are processed, you get a red summary and a `y/N` confirm before any file is touched.

```bash
bigfiles dupes ~/Downloads --delete
```

Safety guarantees:

- Per-group single-choice picker — you can only delete by *not picking* one to keep
- Every group offers a "skip — keep all" option; `Esc` also skips
- Always keeps ≥1 copy per group (it's structurally impossible to empty a group)
- No deletion happens until the final `y/N` confirm; default is **No**
- Files are re-stat'd immediately before removal; non-regular files (symlinks, sockets, devices) are refused
- Files are removed permanently — they do **not** go to Trash

Note that dupes are only ever paired *within* the scan root. If two copies live in separate trees, scan a common parent.

### Delete stale files (interactive)

`bigfiles delete` shows you every file older than `--stale-years` (default 2) in an interactive checklist. You tick which ones to remove, see a confirmation summary, and only then are files deleted. **Files are removed permanently — they do not go to Trash.**

```bash
bigfiles delete ~/Downloads --stale-years 3
```

The flow: list → tick boxes (Space) → Enter → review summary → type `y` to confirm. Hit Ctrl-C any time to bail.

### Flags (global)

| Flag | Default | Description |
|---|---|---|
| `<PATH>` | `.` | Directory to scan |
| `-s, --stale-years <N>` | `2` | Flag files not modified in this many years as stale |
| `-H, --skip-hidden` | off | Skip dotfiles and dot-directories |
| `-d, --depth <N>` | unlimited | Limit traversal depth (1 = only files directly in root) |
| `--no-ignore` | off | Do not respect `.gitignore` / `.ignore` files |
| `--no-pager` | off | Don't auto-page output through `$PAGER` |
| `-e, --exclude <GLOB>` | none | Skip files/dirs matching this glob; repeatable |
| `--units <STYLE>` | `default` | Byte unit style: `default` (1024, KB/MB), `iec` (1024, KiB/MiB), `si` (1000, KB/MB) |
| `--color <WHEN>` | `auto` | Color output: `auto`, `always`, `never`. Also respects `NO_COLOR`. |
| `-t, --top <N>` | off | Show N largest files per category (default scan only) |
| `-j, --json` | off | Emit raw JSON (default scan only) |

### Pager

When stdout is a real terminal, bigfiles auto-pages output through `$PAGER` (default `less -FRX`) — same UX as `git log`. Short output passes through instantly thanks to `-F`; long output (e.g. `bigfiles ~ --top 20`) opens scrollable. Use arrow keys / `/` to search / `q` to quit.

The pager is automatically skipped when:
- output is piped (`bigfiles ... | jq` works as expected)
- `--json` is set
- the `delete` subcommand is running (interactive)
- `--no-pager` is passed

## Example output

```
  bigfiles 8.18 GB  /Users/you/Downloads

  category           size                            files    stale
  ────────────────────────────────────────────────────────────────────────
  video           3.30 GB  ██████████                   45
  archives        2.81 GB  ████████                     44
  documents       1.23 GB  ███                         362
  audio         410.3 MB   █                            29
  images        326.9 MB                               300    ⚠ 91.9 MB (12 files)
  other         115.5 MB                               358    ⚠ 2.5 MB (302 files)
  code          721.3 KB                                25    ⚠ 26.0 KB (14 files)
```

## How "stale" is detected

bigfiles uses the file's **modified time** (`mtime`), not access time. Many filesystems disable access-time updates by default (Linux `noatime`, modern macOS volumes), so `atime` is unreliable for staleness. `mtime` is updated whenever a file's contents change, which is a better signal for "this file is forgotten."

## Project layout

```
src/
  main.rs        # CLI entry, subcommand dispatch, clap styles
  walker.rs      # Parallel directory traversal, file collection, inode capture
  classifier.rs  # Extension → category mapping
  analyzer.rs    # Grouping, sorting, stale detection
  renderer.rs    # Default scan output
  dupes.rs       # Duplicate detection (parallel, hardlink-aware) + interactive delete
  delete.rs      # Interactive stale-file deletion
  format.rs      # Shared byte-size formatter
```

## Platform notes

- **Linux & macOS**: full feature set, including hardlink-aware dupe detection and pager auto-launch.
- **Windows**: builds and runs cleanly via `cargo build --release` (CI covers `windows-latest`). Two caveats:
  - **Pager is disabled** on Windows (there's no portable `less`). Output prints straight to stdout — pipe to `more` or use Windows Terminal's scrollback. The `--no-pager` flag is a no-op there.
  - **Hardlink detection is currently inactive** — the inode/file-index API is nightly-only on `std`. Dupe detection still works, but hardlinks are treated as separate entries instead of being collapsed.

## Caveats

- **Deletion is permanent** for both `delete` and `dupes --delete` — nothing goes to Trash. The interactive flow exists precisely to keep that decision explicit; there is no `--force` or non-interactive delete mode by design.
- **Dupe pairing is relative to the scan root.** If two copies live in separate trees (e.g. `~/A/file` and `~/B/file`), running `bigfiles ~/A dupes` won't find them. Scan a common parent.
- **`--top` and `--json` only apply to the default scan.** They're accepted but ignored under `dupes`/`delete` (a stderr note is printed).
- **Symlinks are skipped entirely.** If you rely on symlink farms for organization, walking through them isn't supported — point bigfiles at the real paths.

## Future ideas

- Per-directory breakdown ("top 10 heaviest subdirectories")
- `--watch` mode that re-scans on an interval
- A full TUI with `ratatui` (expand/collapse categories, arrow-key navigation)
- Persistent index in `~/.cache/bigfiles/` to diff scans over time
- Replace dupes with hardlinks (`--link` mode) instead of deleting

## Stability

Starting with **1.0**, the CLI surface and JSON schema follow semver:

- **CLI flags**: removing a flag, changing its short form, or changing default behavior requires a major version bump. New flags are minor.
- **JSON output**: the `"version": 1` envelope is stable. Breaking changes ship as `"version": 2`. Adding new fields is minor.
- **Exit codes**: `0` success, `1` runtime error, `2` usage error.
- **Internal Rust API**: not stable. Use the binary, not the library crate.

## License

AGPL-3.0-or-later — see [LICENSE](LICENSE).
