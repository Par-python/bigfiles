use crate::walker::FileEntry;
use dialoguer::{theme::ColorfulTheme, Confirm, Select};
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const PARTIAL_HASH_BYTES: u64 = 4096;

pub struct DupeGroup {
    pub size: u64,
    pub paths: Vec<PathBuf>,
}

pub fn find(files: &[FileEntry], min_size: u64) -> Vec<DupeGroup> {
    let mut by_size: HashMap<u64, Vec<&FileEntry>> = HashMap::new();
    for f in files {
        if f.size < min_size {
            continue;
        }
        by_size.entry(f.size).or_default().push(f);
    }

    let mut groups: Vec<DupeGroup> = Vec::new();

    for (size, candidates) in by_size {
        if candidates.len() < 2 {
            continue;
        }

        let mut by_partial: HashMap<[u8; 32], Vec<&FileEntry>> = HashMap::new();
        for f in &candidates {
            if let Some(h) = partial_hash(&f.path) {
                by_partial.entry(h).or_default().push(f);
            }
        }

        for partial_group in by_partial.into_values() {
            if partial_group.len() < 2 {
                continue;
            }

            let mut by_full: HashMap<[u8; 32], Vec<&FileEntry>> = HashMap::new();
            for f in &partial_group {
                if let Some(h) = full_hash(&f.path) {
                    by_full.entry(h).or_default().push(f);
                }
            }

            for full_group in by_full.into_values() {
                if full_group.len() < 2 {
                    continue;
                }
                let mut paths: Vec<PathBuf> = full_group.iter().map(|f| f.path.clone()).collect();
                paths.sort();
                groups.push(DupeGroup { size, paths });
            }
        }
    }

    groups.sort_by(|a, b| {
        let a_waste = a.size * (a.paths.len() as u64 - 1);
        let b_waste = b.size * (b.paths.len() as u64 - 1);
        b_waste.cmp(&a_waste)
    });

    groups
}

fn partial_hash(path: &Path) -> Option<[u8; 32]> {
    let mut file = File::open(path).ok()?;
    let mut buf = vec![0u8; PARTIAL_HASH_BYTES as usize];
    let n = file.read(&mut buf).ok()?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(&buf[..n]);

    let meta = file.metadata().ok()?;
    if meta.len() > PARTIAL_HASH_BYTES * 2 {
        file.seek(SeekFrom::End(-(PARTIAL_HASH_BYTES as i64)))
            .ok()?;
        let n = file.read(&mut buf).ok()?;
        hasher.update(&buf[..n]);
    }

    Some(*hasher.finalize().as_bytes())
}

fn full_hash(path: &Path) -> Option<[u8; 32]> {
    let mut file = File::open(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(*hasher.finalize().as_bytes())
}

pub fn render(groups: &[DupeGroup], root: &Path) {
    println!();
    if groups.is_empty() {
        println!(
            "  {} no duplicates found in {}",
            "✓".green(),
            root.display().dimmed()
        );
        println!();
        return;
    }

    let total_waste: u64 = groups
        .iter()
        .map(|g| g.size * (g.paths.len() as u64 - 1))
        .sum();

    println!(
        "  {} {} {} {} duplicate group{}, {} reclaimable",
        "bigfiles dupes".bold(),
        root.display().dimmed(),
        "·".dimmed(),
        groups.len().to_string().cyan(),
        if groups.len() == 1 { "" } else { "s" },
        format_bytes(total_waste).bold().yellow(),
    );
    println!();

    for (i, g) in groups.iter().enumerate() {
        let waste = g.size * (g.paths.len() as u64 - 1);
        println!(
            "  {} {} copies × {} = {} wasted",
            format!("[{}]", i + 1).dimmed(),
            g.paths.len().to_string().cyan(),
            format_bytes(g.size),
            format_bytes(waste).yellow(),
        );
        for p in &g.paths {
            println!("      {}", p.display());
        }
        println!();
    }
}

pub fn delete_interactive(groups: &[DupeGroup], root: &Path) -> io::Result<()> {
    println!();
    if groups.is_empty() {
        println!(
            "  {} no duplicates found in {}",
            "✓".green(),
            root.display().dimmed()
        );
        println!();
        return Ok(());
    }

    println!(
        "  {} {} {} {} duplicate group{} — pick one copy to KEEP per group",
        "bigfiles dupes --delete".bold(),
        root.display().dimmed(),
        "·".dimmed(),
        groups.len().to_string().cyan(),
        if groups.len() == 1 { "" } else { "s" },
    );
    println!(
        "  {}",
        "Use ↑/↓ to choose what to keep, Enter to confirm. Esc to skip a group.".dimmed()
    );
    println!();

    let mut to_delete: Vec<PathBuf> = Vec::new();
    let mut delete_size: u64 = 0;

    for (i, g) in groups.iter().enumerate() {
        let waste = g.size * (g.paths.len() as u64 - 1);
        println!(
            "  {} {} copies × {} = {} reclaimable",
            format!("[{}/{}]", i + 1, groups.len()).dimmed(),
            g.paths.len().to_string().cyan(),
            format_bytes(g.size),
            format_bytes(waste).yellow(),
        );

        let items: Vec<String> = g.paths.iter().map(|p| p.display().to_string()).collect();
        let mut skip_items = items.clone();
        skip_items.push("[skip this group — keep all]".to_string());

        let pick = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Keep which copy?")
            .items(&skip_items)
            .default(0)
            .interact_opt()
            .map_err(io::Error::other)?;

        let Some(idx) = pick else {
            println!("  {}", "skipped.".dimmed());
            println!();
            continue;
        };

        if idx == items.len() {
            println!("  {}", "skipped.".dimmed());
            println!();
            continue;
        }

        for (j, p) in g.paths.iter().enumerate() {
            if j != idx {
                to_delete.push(p.clone());
                delete_size += g.size;
            }
        }
        println!();
    }

    if to_delete.is_empty() {
        println!("  {}", "Nothing selected. No files deleted.".dimmed());
        println!();
        return Ok(());
    }

    println!();
    println!(
        "  About to {} {} duplicate cop{} ({}):",
        "PERMANENTLY DELETE".red().bold(),
        to_delete.len().to_string().red().bold(),
        if to_delete.len() == 1 { "y" } else { "ies" },
        format_bytes(delete_size).red().bold(),
    );
    for p in &to_delete {
        println!("    {}", p.display());
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
        println!();
        return Ok(());
    }

    let mut deleted = 0usize;
    let mut failed = 0usize;
    let mut freed = 0u64;
    for p in &to_delete {
        let meta = match fs::symlink_metadata(p) {
            Ok(m) => m,
            Err(e) => {
                failed += 1;
                eprintln!("  failed (stat): {} ({})", p.display(), e);
                continue;
            }
        };
        if !meta.file_type().is_file() {
            failed += 1;
            eprintln!(
                "  refused (not a regular file — symlink or special): {}",
                p.display()
            );
            continue;
        }
        let size = meta.len();
        match fs::remove_file(p) {
            Ok(()) => {
                deleted += 1;
                freed += size;
            }
            Err(e) => {
                failed += 1;
                eprintln!("  failed: {} ({})", p.display(), e);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::walker::FileEntry;
    use std::fs;
    use std::io::Write;
    use std::time::SystemTime;
    use tempfile::tempdir;

    fn write_file(path: &Path, bytes: &[u8]) -> FileEntry {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        f.write_all(bytes).unwrap();
        FileEntry {
            path: path.to_path_buf(),
            size: bytes.len() as u64,
            extension: path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("none")
                .to_lowercase(),
            modified: SystemTime::now(),
        }
    }

    #[test]
    fn identical_files_form_a_group() {
        let dir = tempdir().unwrap();
        let payload = vec![7u8; 8192];
        let a = write_file(&dir.path().join("a.bin"), &payload);
        let b = write_file(&dir.path().join("b.bin"), &payload);
        let groups = find(&[a, b], 0);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].paths.len(), 2);
        assert_eq!(groups[0].size, 8192);
    }

    #[test]
    fn different_content_same_size_is_not_a_dupe() {
        let dir = tempdir().unwrap();
        let a = write_file(&dir.path().join("a.bin"), &vec![1u8; 8192]);
        let b = write_file(&dir.path().join("b.bin"), &vec![2u8; 8192]);
        let groups = find(&[a, b], 0);
        assert!(groups.is_empty());
    }

    #[test]
    fn min_size_filter_excludes_small_files() {
        let dir = tempdir().unwrap();
        let a = write_file(&dir.path().join("a.bin"), b"hi");
        let b = write_file(&dir.path().join("b.bin"), b"hi");
        let groups = find(&[a, b], 1024);
        assert!(groups.is_empty());
    }

    #[test]
    fn three_copies_grouped_together() {
        let dir = tempdir().unwrap();
        let payload = vec![42u8; 4096];
        let a = write_file(&dir.path().join("a.bin"), &payload);
        let b = write_file(&dir.path().join("b.bin"), &payload);
        let c = write_file(&dir.path().join("c.bin"), &payload);
        let groups = find(&[a, b, c], 0);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].paths.len(), 3);
    }

    #[test]
    fn singletons_are_not_reported() {
        let dir = tempdir().unwrap();
        let a = write_file(&dir.path().join("lonely.bin"), &vec![9u8; 2048]);
        let groups = find(&[a], 0);
        assert!(groups.is_empty());
    }

    #[test]
    fn groups_sorted_by_wasted_space_desc() {
        let dir = tempdir().unwrap();
        // Group A: 2 copies × 1024 = 1024 wasted
        let small = vec![1u8; 1024];
        let a1 = write_file(&dir.path().join("small1.bin"), &small);
        let a2 = write_file(&dir.path().join("small2.bin"), &small);
        // Group B: 2 copies × 8192 = 8192 wasted (should rank first)
        let big = vec![2u8; 8192];
        let b1 = write_file(&dir.path().join("big1.bin"), &big);
        let b2 = write_file(&dir.path().join("big2.bin"), &big);

        let groups = find(&[a1, a2, b1, b2], 0);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].size, 8192);
        assert_eq!(groups[1].size, 1024);
    }

    #[test]
    fn handles_large_file_with_tail_hashing() {
        let dir = tempdir().unwrap();
        // Files larger than 2 * PARTIAL_HASH_BYTES exercise the tail-hash branch.
        let size = (PARTIAL_HASH_BYTES * 3) as usize;
        let mut payload = vec![0u8; size];
        for (i, b) in payload.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        let a = write_file(&dir.path().join("a.bin"), &payload);
        let b = write_file(&dir.path().join("b.bin"), &payload);

        // Same head and tail, different middle → must NOT be reported as dupes.
        let mut diff_middle = payload.clone();
        let mid = size / 2;
        diff_middle[mid] ^= 0xFF;
        let c = write_file(&dir.path().join("c.bin"), &diff_middle);

        let groups = find(&[a, b, c], 0);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].paths.len(), 2);
    }

    #[test]
    fn paths_within_group_are_sorted() {
        let dir = tempdir().unwrap();
        let payload = vec![3u8; 4096];
        let z = write_file(&dir.path().join("z.bin"), &payload);
        let a = write_file(&dir.path().join("a.bin"), &payload);
        let m = write_file(&dir.path().join("m.bin"), &payload);
        let groups = find(&[z, a, m], 0);
        assert_eq!(groups.len(), 1);
        let names: Vec<_> = groups[0]
            .paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a.bin", "m.bin", "z.bin"]);
    }
}
