use crate::walker::FileEntry;
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
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
