use crate::analyzer::CategorySummary;
use crate::classifier::categorize;
use crate::format::bytes as format_bytes;
use crate::walker::FileEntry;
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

pub fn render(summaries: &[CategorySummary], total: u64, skipped: usize, root: &Path) {
    print!("{}", render_to_string(summaries, total, skipped, root));
}

pub fn render_to_string(
    summaries: &[CategorySummary],
    total: u64,
    skipped: usize,
    root: &Path,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  {} {}  {}",
        "bigfiles".bold(),
        format_bytes(total).bold().cyan(),
        root.display().dimmed()
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  {:<14} {:>9}  {:<24} {:>7}    {}",
        "category".dimmed(),
        "size".dimmed(),
        "".dimmed(),
        "files".dimmed(),
        "stale".dimmed(),
    );
    let _ = writeln!(out, "  {}", "─".repeat(72).dimmed());

    for s in summaries {
        let pct = if total > 0 {
            (s.total_size as f64 / total as f64 * 100.0) as usize
        } else {
            0
        };
        let bar_len = pct / 4;
        let bar = "█".repeat(bar_len);

        let stale_note = if s.stale_size > 0 {
            format!("⚠ {} ({} files)", format_bytes(s.stale_size), s.stale_count)
        } else {
            String::new()
        };

        let pad = 14usize.saturating_sub(s.category.chars().count());
        let cat_colored = paint_category(&s.category);
        let _ = writeln!(
            out,
            "  {}{} {:>9}  {:<24} {:>7}    {}",
            cat_colored,
            " ".repeat(pad),
            format_bytes(s.total_size),
            bar,
            s.file_count,
            stale_note.yellow(),
        );
    }
    let _ = writeln!(out);
    if skipped > 0 {
        let _ = writeln!(
            out,
            "  {} {} {}",
            "note:".dimmed(),
            skipped.to_string().yellow(),
            "entries skipped (permission denied or unreadable)".dimmed()
        );
        let _ = writeln!(out);
    }
    out
}

fn paint_category(cat: &str) -> String {
    match cat {
        "video" => cat.red().to_string(),
        "images" => cat.magenta().to_string(),
        "archives" => cat.yellow().to_string(),
        "audio" => cat.cyan().to_string(),
        "documents" => cat.blue().to_string(),
        "code" => cat.green().to_string(),
        "junk" => cat.bright_red().to_string(),
        "no extension" => cat.dimmed().to_string(),
        _ => cat.white().to_string(),
    }
}

pub fn render_top(files: &[FileEntry], n: usize) {
    print!("{}", render_top_to_string(files, n));
}

pub fn render_top_to_string(files: &[FileEntry], n: usize) -> String {
    let mut out = String::new();
    if n == 0 || files.is_empty() {
        return out;
    }

    let mut by_cat: HashMap<&'static str, Vec<&FileEntry>> = HashMap::new();
    for f in files {
        let cat = categorize(&f.extension);
        by_cat.entry(cat).or_default().push(f);
    }

    let mut cats: Vec<(&'static str, Vec<&FileEntry>)> = by_cat.into_iter().collect();
    cats.sort_by(|a, b| {
        let a_size: u64 = a.1.iter().map(|f| f.size).sum();
        let b_size: u64 = b.1.iter().map(|f| f.size).sum();
        b_size.cmp(&a_size)
    });

    let _ = writeln!(
        out,
        "  {} largest {} per category",
        "top".bold(),
        n.to_string().cyan()
    );
    let _ = writeln!(out, "  {}", "─".repeat(72).dimmed());

    for (cat, mut entries) in cats {
        entries.sort_by_key(|f| std::cmp::Reverse(f.size));
        let cat_colored = paint_category(cat);
        let _ = writeln!(out, "  {}", cat_colored);
        for f in entries.iter().take(n) {
            let _ = writeln!(
                out,
                "    {:>10}  {}",
                format_bytes(f.size),
                f.path.display().to_string().dimmed()
            );
        }
        let _ = writeln!(out);
    }
    out
}

#[cfg(test)]
fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for nc in chars.by_ref() {
                if nc.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::CategorySummary;
    use crate::walker::FileEntry;
    use std::path::PathBuf;
    use std::time::SystemTime;

    fn entry(path: &str, size: u64, ext: &str) -> FileEntry {
        FileEntry {
            path: PathBuf::from(path),
            size,
            extension: ext.to_string(),
            modified: SystemTime::UNIX_EPOCH,
            inode: None,
        }
    }

    fn summaries() -> Vec<CategorySummary> {
        vec![
            CategorySummary {
                category: "video".to_string(),
                total_size: 3_500_000_000,
                file_count: 12,
                stale_size: 0,
                stale_count: 0,
            },
            CategorySummary {
                category: "images".to_string(),
                total_size: 800_000_000,
                file_count: 230,
                stale_size: 100_000_000,
                stale_count: 14,
            },
            CategorySummary {
                category: "code".to_string(),
                total_size: 50_000,
                file_count: 30,
                stale_size: 0,
                stale_count: 0,
            },
        ]
    }

    #[test]
    fn snapshot_default_render() {
        let out = render_to_string(&summaries(), 4_300_000_000, 3, Path::new("/scan/root"));
        insta::assert_snapshot!(strip_ansi(&out));
    }

    #[test]
    fn snapshot_default_render_no_skipped() {
        let out = render_to_string(&summaries(), 4_300_000_000, 0, Path::new("/scan/root"));
        insta::assert_snapshot!(strip_ansi(&out));
    }

    #[test]
    fn snapshot_render_top() {
        let files = vec![
            entry("/scan/root/movie.mkv", 2_000_000_000, "mkv"),
            entry("/scan/root/clip.mp4", 500_000_000, "mp4"),
            entry("/scan/root/photo.jpg", 4_000_000, "jpg"),
            entry("/scan/root/main.rs", 1024, "rs"),
        ];
        let out = render_top_to_string(&files, 2);
        insta::assert_snapshot!(strip_ansi(&out));
    }

    #[test]
    fn render_top_empty_returns_empty() {
        assert_eq!(render_top_to_string(&[], 5), "");
        assert_eq!(render_top_to_string(&[entry("/a", 1, "txt")], 0), "");
    }
}
