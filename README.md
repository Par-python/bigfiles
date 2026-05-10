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

# Skip hidden files and dirs
bigfiles ~ --skip-hidden

# Treat anything not modified in 5+ years as stale (default: 2)
bigfiles ~/Documents --stale-years 5

# Pipe JSON into jq
bigfiles ~/Movies --json | jq '.[] | select(.stale_size > 1000000000)'
```

### Flags

| Flag | Default | Description |
|---|---|---|
| `<PATH>` | `.` | Directory to scan |
| `-s, --stale-years <N>` | `2` | Flag files not modified in this many years as stale |
| `-H, --skip-hidden` | off | Skip dotfiles and dot-directories |
| `-j, --json` | off | Emit raw JSON instead of the table |

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
  main.rs        # CLI entry, arg parsing
  walker.rs      # Directory traversal, file collection
  classifier.rs  # Extension → category mapping
  analyzer.rs    # Grouping, sorting, stale detection
  renderer.rs    # Terminal output + byte formatting
```

## Future ideas

- `--top N` to list the largest files within each category
- `--delete` flag with an interactive confirmation list for stale files
- `--ignore` glob patterns (respect `.gitignore` via the `ignore` crate)
- A full TUI with `ratatui` (expand/collapse categories, arrow-key navigation)

## License

MIT
