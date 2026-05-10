use crate::analyzer::CategorySummary;
use crate::classifier::categorize;
use crate::format::bytes as format_bytes;
use crate::walker::FileEntry;
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::path::Path;

pub fn render(summaries: &[CategorySummary], total: u64, skipped: usize, root: &Path) {
    println!();
    println!(
        "  {} {}  {}",
        "bigfiles".bold(),
        format_bytes(total).bold().cyan(),
        root.display().dimmed()
    );
    println!();
    println!(
        "  {:<14} {:>9}  {:<24} {:>7}    {}",
        "category".dimmed(),
        "size".dimmed(),
        "".dimmed(),
        "files".dimmed(),
        "stale".dimmed(),
    );
    println!("  {}", "─".repeat(72).dimmed());

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
        println!(
            "  {}{} {:>9}  {:<24} {:>7}    {}",
            cat_colored,
            " ".repeat(pad),
            format_bytes(s.total_size),
            bar,
            s.file_count,
            stale_note.yellow(),
        );
    }
    println!();
    if skipped > 0 {
        println!(
            "  {} {} {}",
            "note:".dimmed(),
            skipped.to_string().yellow(),
            "entries skipped (permission denied or unreadable)".dimmed()
        );
        println!();
    }
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
    if n == 0 || files.is_empty() {
        return;
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

    println!(
        "  {} largest {} per category",
        "top".bold(),
        n.to_string().cyan()
    );
    println!("  {}", "─".repeat(72).dimmed());

    for (cat, mut entries) in cats {
        entries.sort_by_key(|f| std::cmp::Reverse(f.size));
        let cat_colored = paint_category(cat);
        println!("  {}", cat_colored);
        for f in entries.iter().take(n) {
            println!(
                "    {:>10}  {}",
                format_bytes(f.size),
                f.path.display().to_string().dimmed()
            );
        }
        println!();
    }
}
