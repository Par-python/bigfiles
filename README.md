# bigfiles

A small Rust CLI that walks a directory, groups files by type, flags stale ones, and renders a color-coded summary in the terminal.

## What it does

- Walks a directory tree and collects file sizes, extensions, and modified timestamps
- Groups files into categories: video, images, archives, audio, documents, code, junk, other
- Flags files not modified in the last N years as stale
- Renders a color-coded table with size bars, or emits JSON for piping
- Reports how many entries were skipped due to permission errors

## Install

Requires Rust (install via [rustup](https://rustup.rs)).

```bash
git clone <this repo>
cd bigfiles
cargo install --path .
```

Or just build the release binary:

```bash
cargo build --release
./target/release/bigfiles --help
```

## Usage

```bash
# Scan current directory
bigfiles

# Scan a specific path
bigfiles ~/Downloads

# Skip hidden files and dirs, only descend 3 levels
bigfiles ~ --skip-hidden --depth 3

# Treat anything not modified in 5+ years as stale (default: 2)
bigfiles ~/Documents --stale-years 5

# Pipe JSON into jq
bigfiles ~/Movies --json | jq '.[] | select(.stale_size > 1000000000)'
```

### Find duplicate files

`bigfiles dupes` finds files with identical content. It uses a fast three-stage check: group by size → hash first/last 4 KB → full BLAKE3 hash on remaining candidates. Almost no false positives, fast on large trees.

```bash
# Find dupes >= 1 MB in Downloads
bigfiles dupes ~/Downloads --min-size 1048576

# Default min-size is 1 KB; tune as needed
bigfiles dupes ~/Documents --min-size 1
```

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
| `-j, --json` | off | Emit raw JSON (default scan only) |

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
  main.rs        # CLI entry, subcommand dispatch
  walker.rs      # Directory traversal, file collection
  classifier.rs  # Extension → category mapping
  analyzer.rs    # Grouping, sorting, stale detection
  renderer.rs    # Default scan output
  dupes.rs       # Duplicate detection + rendering
  delete.rs      # Interactive stale-file deletion
```

## Future ideas

- `--top N` to list the largest files within each category
- `--ignore` glob patterns (respect `.gitignore` via the `ignore` crate)
- Parallel walk with `rayon` + `jwalk` for large trees
- A full TUI with `ratatui` (expand/collapse categories, arrow-key navigation)
- Persistent index in `~/.cache/bigfiles/` to diff scans over time

## License

MIT
