use crate::analyzer::CategorySummary;
use crate::format::bytes as format_bytes;
use crate::walker::FileEntry;
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, SystemTime};

const INSTALLER_EXTS: &[&str] = &["dmg", "pkg", "iso", "exe", "msi", "deb", "rpm"];
const TOP_N_CONCENTRATION: usize = 10;
const TOP_EXTENSIONS: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Info,
    Notable,
    Heavy,
}

#[derive(Debug, Clone)]
pub struct Insight {
    pub headline: String,
    pub severity: Severity,
}

pub fn analyze(
    files: &[FileEntry],
    summaries: &[CategorySummary],
    total_size: u64,
    stale_years: u64,
) -> Vec<Insight> {
    let mut out = Vec::new();
    if total_size == 0 {
        return out;
    }

    if let Some(i) = heaviest_category(summaries, total_size) {
        out.push(i);
    }
    out.extend(top_extensions(files, total_size));
    if let Some(i) = installer_junk(files, total_size) {
        out.push(i);
    }
    if let Some(i) = top_n_concentration(files, total_size) {
        out.push(i);
    }
    if let Some(i) = stale_concentration(files, total_size, stale_years) {
        out.push(i);
    }
    out
}

fn heaviest_category(summaries: &[CategorySummary], total: u64) -> Option<Insight> {
    let top = summaries.iter().max_by_key(|s| s.total_size)?;
    if top.total_size == 0 {
        return None;
    }
    let pct = percent(top.total_size, total);
    let severity = if pct >= 40.0 {
        Severity::Heavy
    } else if pct >= 20.0 {
        Severity::Notable
    } else {
        Severity::Info
    };
    Some(Insight {
        headline: format!(
            "{} is your heaviest category: {} across {} files ({:.0}% of total)",
            top.category,
            format_bytes(top.total_size),
            count_str(top.file_count),
            pct
        ),
        severity,
    })
}

fn top_extensions(files: &[FileEntry], total: u64) -> Vec<Insight> {
    let mut by_ext: HashMap<&str, (u64, usize)> = HashMap::new();
    for f in files {
        let entry = by_ext.entry(f.extension.as_str()).or_insert((0, 0));
        entry.0 += f.size;
        entry.1 += 1;
    }
    let mut ranked: Vec<(&str, u64, usize)> =
        by_ext.into_iter().map(|(e, (s, c))| (e, s, c)).collect();
    ranked.sort_by_key(|(_, s, _)| std::cmp::Reverse(*s));

    let mut out = Vec::new();
    for (ext, size, count) in ranked.into_iter().take(TOP_EXTENSIONS) {
        if size == 0 {
            continue;
        }
        let pct = percent(size, total);
        if pct < 1.0 {
            continue;
        }
        let label = if ext == "none" {
            "(no extension)".to_string()
        } else {
            format!(".{}", ext)
        };
        let severity = if pct >= 25.0 {
            Severity::Heavy
        } else if pct >= 10.0 {
            Severity::Notable
        } else {
            Severity::Info
        };
        out.push(Insight {
            headline: format!(
                "{} files account for {} ({:.0}% of total, {} files)",
                label,
                format_bytes(size),
                pct,
                count_str(count)
            ),
            severity,
        });
    }
    out
}

fn installer_junk(files: &[FileEntry], total: u64) -> Option<Insight> {
    let mut size = 0u64;
    let mut count = 0usize;
    for f in files {
        if INSTALLER_EXTS.contains(&f.extension.as_str()) {
            size += f.size;
            count += 1;
        }
    }
    if count == 0 || size == 0 {
        return None;
    }
    let pct = percent(size, total);
    let severity = if pct >= 10.0 {
        Severity::Notable
    } else {
        Severity::Info
    };
    Some(Insight {
        headline: format!(
            "Installer files (.dmg/.pkg/.iso/.exe/.msi/.deb/.rpm): {} across {} files — usually safe to delete after use",
            format_bytes(size),
            count_str(count)
        ),
        severity,
    })
}

fn top_n_concentration(files: &[FileEntry], total: u64) -> Option<Insight> {
    if files.len() <= TOP_N_CONCENTRATION {
        return None;
    }
    let mut sizes: Vec<u64> = files.iter().map(|f| f.size).collect();
    sizes.sort_unstable_by_key(|s| std::cmp::Reverse(*s));
    let top_sum: u64 = sizes.iter().take(TOP_N_CONCENTRATION).sum();
    if top_sum == 0 {
        return None;
    }
    let pct = percent(top_sum, total);
    if pct < 5.0 {
        return None;
    }
    let severity = if pct >= 50.0 {
        Severity::Heavy
    } else if pct >= 25.0 {
        Severity::Notable
    } else {
        Severity::Info
    };
    Some(Insight {
        headline: format!(
            "Your top {} files alone are {} ({:.0}% of total) — concentrated weight",
            TOP_N_CONCENTRATION,
            format_bytes(top_sum),
            pct
        ),
        severity,
    })
}

fn stale_concentration(files: &[FileEntry], total: u64, stale_years: u64) -> Option<Insight> {
    let now = SystemTime::now();
    let threshold = Duration::from_secs(stale_years * 365 * 24 * 60 * 60);
    let mut stale_size = 0u64;
    let mut stale_count = 0usize;
    for f in files {
        if let Ok(age) = now.duration_since(f.modified) {
            if age > threshold {
                stale_size += f.size;
                stale_count += 1;
            }
        }
    }
    if stale_count == 0 || stale_size == 0 {
        return None;
    }
    let pct = percent(stale_size, total);
    let severity = if pct >= 40.0 {
        Severity::Heavy
    } else if pct >= 20.0 {
        Severity::Notable
    } else {
        Severity::Info
    };
    Some(Insight {
        headline: format!(
            "{} ({:.0}% of total) is stale — older than {} year{} across {} files",
            format_bytes(stale_size),
            pct,
            stale_years,
            if stale_years == 1 { "" } else { "s" },
            count_str(stale_count)
        ),
        severity,
    })
}

pub fn render(insights: &[Insight], total: u64, root: &Path) {
    print!("{}", render_to_string(insights, total, root));
}

pub fn render_to_string(insights: &[Insight], total: u64, root: &Path) -> String {
    let mut out = String::new();
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  {} {}  {}",
        "bigfiles audit".bold(),
        format_bytes(total).bold().cyan(),
        root.display().dimmed()
    );
    let _ = writeln!(out);

    if insights.is_empty() {
        let _ = writeln!(out, "  {}", "Nothing notable to report.".dimmed());
        let _ = writeln!(out);
        return out;
    }

    for i in insights {
        let bullet = match i.severity {
            Severity::Heavy => "!".red().bold().to_string(),
            Severity::Notable => "•".yellow().bold().to_string(),
            Severity::Info => "·".dimmed().to_string(),
        };
        let _ = writeln!(out, "  {} {}", bullet, i.headline);
    }
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  {}",
        "Run `bigfiles` for the full category breakdown, or `bigfiles dupes` to find duplicates."
            .dimmed()
    );
    let _ = writeln!(out);
    out
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    (part as f64 / whole as f64) * 100.0
}

fn count_str(n: usize) -> String {
    n.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    fn fe(path: &str, size: u64, ext: &str, age_secs: u64) -> FileEntry {
        FileEntry {
            path: PathBuf::from(path),
            size,
            extension: ext.to_string(),
            modified: SystemTime::now() - Duration::from_secs(age_secs),
            inode: None,
        }
    }

    fn cs(category: &str, total: u64, count: usize) -> CategorySummary {
        CategorySummary {
            category: category.to_string(),
            total_size: total,
            file_count: count,
            stale_size: 0,
            stale_count: 0,
        }
    }

    #[test]
    fn empty_total_yields_no_insights() {
        let v = analyze(&[], &[], 0, 2);
        assert!(v.is_empty());
    }

    #[test]
    fn heaviest_category_is_reported() {
        let summaries = vec![cs("video", 800, 10), cs("audio", 200, 50)];
        let files = vec![fe("a.mp4", 800, "mp4", 0), fe("b.mp3", 200, "mp3", 0)];
        let v = analyze(&files, &summaries, 1000, 2);
        let h = v.iter().find(|i| i.headline.contains("heaviest")).unwrap();
        assert!(h.headline.contains("video"));
        assert!(h.headline.contains("80%"));
        assert_eq!(h.severity, Severity::Heavy);
    }

    #[test]
    fn installer_junk_detected() {
        let files = vec![
            fe("a.dmg", 500, "dmg", 0),
            fe("b.pkg", 300, "pkg", 0),
            fe("c.txt", 100, "txt", 0),
        ];
        let summaries = vec![cs("archives", 800, 2), cs("other", 100, 1)];
        let v = analyze(&files, &summaries, 900, 2);
        assert!(v.iter().any(|i| i.headline.contains("Installer files")));
    }

    #[test]
    fn stale_concentration_uses_threshold() {
        let three_years = 3 * 365 * 24 * 60 * 60;
        let files = vec![
            fe("old.mp4", 600, "mp4", three_years),
            fe("new.mp4", 400, "mp4", 0),
        ];
        let summaries = vec![cs("video", 1000, 2)];
        let v = analyze(&files, &summaries, 1000, 2);
        let stale = v.iter().find(|i| i.headline.contains("stale")).unwrap();
        assert!(stale.headline.contains("60%"));
    }

    #[test]
    fn top_n_concentration_skipped_for_few_files() {
        let files = vec![fe("a", 100, "txt", 0), fe("b", 100, "txt", 0)];
        let summaries = vec![cs("other", 200, 2)];
        let v = analyze(&files, &summaries, 200, 2);
        assert!(!v.iter().any(|i| i.headline.contains("top 10")));
    }

    #[test]
    fn top_extensions_reported() {
        let mut files = Vec::new();
        for _ in 0..20 {
            files.push(fe("v.mp4", 100, "mp4", 0));
        }
        for _ in 0..5 {
            files.push(fe("a.mp3", 10, "mp3", 0));
        }
        let summaries = vec![cs("video", 2000, 20), cs("audio", 50, 5)];
        let v = analyze(&files, &summaries, 2050, 2);
        assert!(v.iter().any(|i| i.headline.contains(".mp4")));
    }

    #[test]
    fn renders_without_panicking() {
        let summaries = vec![cs("video", 800, 10)];
        let files = vec![fe("a.mp4", 800, "mp4", 0)];
        let insights = analyze(&files, &summaries, 1000, 2);
        let s = render_to_string(&insights, 1000, Path::new("/test"));
        assert!(s.contains("bigfiles audit"));
    }
}
