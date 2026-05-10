use std::fs;
use std::io;
use std::time::{Duration, SystemTime};
use dialoguer::{theme::ColorfulTheme, Confirm, MultiSelect};
use owo_colors::OwoColorize;
use crate::walker::FileEntry;

pub fn run(files: &[FileEntry], stale_years: u64) -> io::Result<()> {
    let now = SystemTime::now();
    let threshold = Duration::from_secs(stale_years * 365 * 24 * 60 * 60);

    let mut stale: Vec<&FileEntry> = files
        .iter()
        .filter(|f| {
            now.duration_since(f.modified)
                .map(|age| age > threshold)
                .unwrap_or(false)
        })
        .collect();

    if stale.is_empty() {
        println!();
        println!(
            "  {} no files older than {} years found.",
            "✓".green(),
            stale_years
        );
        println!();
        return Ok(());
    }

    stale.sort_by(|a, b| b.size.cmp(&a.size));

    let total_size: u64 = stale.iter().map(|f| f.size).sum();

    println!();
    println!(
        "  {} {} stale files (>{}y old) totaling {}",
        "⚠".yellow().bold(),
        stale.len().to_string().cyan(),
        stale_years,
        format_bytes(total_size).bold().yellow()
    );
    println!(
        "  {}",
        "Tick the files you want to permanently delete. Space to toggle, Enter to confirm.".dimmed()
    );
    println!();

    let labels: Vec<String> = stale
        .iter()
        .map(|f| {
            format!(
                "{:>10}  {}",
                format_bytes(f.size),
                f.path.display()
            )
        })
        .collect();

    let selected = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select files to delete")
        .items(&labels)
        .interact()
        .map_err(io::Error::other)?;

    if selected.is_empty() {
        println!("  {}", "Nothing selected. No files deleted.".dimmed());
        return Ok(());
    }

    let to_delete: Vec<&&FileEntry> = selected.iter().map(|i| &stale[*i]).collect();
    let delete_size: u64 = to_delete.iter().map(|f| f.size).sum();

    println!();
    println!(
        "  About to {} {} files ({}):",
        "PERMANENTLY DELETE".red().bold(),
        to_delete.len().to_string().red().bold(),
        format_bytes(delete_size).red().bold(),
    );
    for f in &to_delete {
        println!("    {}", f.path.display());
    }
    println!();
    println!(
        "  {} {}",
        "Files will not go to Trash.".red(),
        "This cannot be undone.".red().bold()
    );
    println!();

    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Proceed with deletion?")
        .default(false)
        .interact()
        .map_err(io::Error::other)?;

    if !confirmed {
        println!("  {}", "Aborted. No files deleted.".dimmed());
        return Ok(());
    }

    let mut deleted = 0usize;
    let mut failed = 0usize;
    let mut freed = 0u64;
    for f in to_delete {
        match fs::remove_file(&f.path) {
            Ok(()) => {
                deleted += 1;
                freed += f.size;
            }
            Err(e) => {
                failed += 1;
                eprintln!("  failed: {} ({})", f.path.display(), e);
            }
        }
    }

    println!();
    println!(
        "  {} deleted {} files, freed {}",
        "✓".green(),
        deleted.to_string().cyan(),
        format_bytes(freed).cyan()
    );
    if failed > 0 {
        println!(
            "  {} {} files could not be deleted",
            "✗".red(),
            failed.to_string().red()
        );
    }
    println!();
    Ok(())
}

fn format_bytes(b: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if b < KB {
        format!("{} B", b)
    } else if b < MB {
        format!("{:.1} KB", b as f64 / KB as f64)
    } else if b < GB {
        format!("{:.1} MB", b as f64 / MB as f64)
    } else {
        format!("{:.2} GB", b as f64 / GB as f64)
    }
}
