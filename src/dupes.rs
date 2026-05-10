use crate::format::bytes as format_bytes;
use crate::walker::{FileEntry, InodeKey};
use dialoguer::{theme::ColorfulTheme, Confirm, Select};
use owo_colors::OwoColorize;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const PARTIAL_HASH_BYTES: u64 = 4096;

pub struct DupeEntry {
    pub paths: Vec<PathBuf>,
}

impl DupeEntry {
    pub fn primary_path(&self) -> &Path {
        &self.paths[0]
    }
}

pub struct DupeGroup {
    pub size: u64,
    pub entries: Vec<DupeEntry>,
}

impl DupeGroup {
    pub fn reclaimable(&self) -> u64 {
        self.size * (self.entries.len() as u64 - 1)
    }
}

pub fn find(files: &[FileEntry], min_size: u64) -> Vec<DupeGroup> {
    let by_size = group_by_size(files, min_size);
    let mut groups: Vec<DupeGroup> = by_size
        .into_par_iter()
        .flat_map_iter(|(size, candidates)| process_size_bucket(size, candidates).into_iter())
        .collect();

    groups.sort_by_key(|g| std::cmp::Reverse(g.reclaimable()));
    groups
}

fn group_by_size(files: &[FileEntry], min_size: u64) -> HashMap<u64, Vec<&FileEntry>> {
    let mut by_size: HashMap<u64, Vec<&FileEntry>> = HashMap::new();
    for f in files {
        if f.size < min_size {
            continue;
        }
        by_size.entry(f.size).or_default().push(f);
    }
    by_size.retain(|_, v| v.len() >= 2);
    by_size
}

fn process_size_bucket(size: u64, candidates: Vec<&FileEntry>) -> Vec<DupeGroup> {
    let by_inode = collapse_hardlinks(&candidates);
    if by_inode.len() < 2 {
        return Vec::new();
    }
    let by_partial = group_by_partial_hash(&by_inode);
    let mut out = Vec::new();
    for partial_group in by_partial {
        for full_group in group_by_full_hash(&partial_group) {
            let mut entries: Vec<DupeEntry> = full_group;
            entries.sort_by(|a, b| a.primary_path().cmp(b.primary_path()));
            out.push(DupeGroup { size, entries });
        }
    }
    out
}

fn collapse_hardlinks(files: &[&FileEntry]) -> Vec<DupeEntry> {
    let mut by_inode: HashMap<InodeKey, Vec<PathBuf>> = HashMap::new();
    let mut without_inode: Vec<PathBuf> = Vec::new();
    for f in files {
        match f.inode {
            Some(k) => by_inode.entry(k).or_default().push(f.path.clone()),
            None => without_inode.push(f.path.clone()),
        }
    }
    let mut entries: Vec<DupeEntry> = by_inode
        .into_values()
        .map(|mut paths| {
            paths.sort();
            DupeEntry { paths }
        })
        .collect();
    for p in without_inode {
        entries.push(DupeEntry { paths: vec![p] });
    }
    entries
}

fn group_by_partial_hash(entries: &[DupeEntry]) -> Vec<Vec<DupeEntry>> {
    let hashed: Vec<([u8; 32], &DupeEntry)> = entries
        .par_iter()
        .filter_map(|e| partial_hash(e.primary_path()).map(|h| (h, e)))
        .collect();

    let mut by_hash: HashMap<[u8; 32], Vec<DupeEntry>> = HashMap::new();
    for (h, e) in hashed {
        by_hash.entry(h).or_default().push(DupeEntry {
            paths: e.paths.clone(),
        });
    }
    by_hash.into_values().filter(|v| v.len() >= 2).collect()
}

fn group_by_full_hash(entries: &[DupeEntry]) -> Vec<Vec<DupeEntry>> {
    let hashed: Vec<([u8; 32], &DupeEntry)> = entries
        .par_iter()
        .filter_map(|e| full_hash(e.primary_path()).map(|h| (h, e)))
        .collect();

    let mut by_hash: HashMap<[u8; 32], Vec<DupeEntry>> = HashMap::new();
    for (h, e) in hashed {
        by_hash.entry(h).or_default().push(DupeEntry {
            paths: e.paths.clone(),
        });
    }
    by_hash.into_values().filter(|v| v.len() >= 2).collect()
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

    let total_waste: u64 = groups.iter().map(|g| g.reclaimable()).sum();

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
        println!(
            "  {} {} copies × {} = {} wasted",
            format!("[{}]", i + 1).dimmed(),
            g.entries.len().to_string().cyan(),
            format_bytes(g.size),
            format_bytes(g.reclaimable()).yellow(),
        );
        for e in &g.entries {
            println!("      {}", e.primary_path().display());
            for hl in e.paths.iter().skip(1) {
                println!(
                    "        {} {}",
                    "↳ hardlink:".dimmed(),
                    hl.display().to_string().dimmed()
                );
            }
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
        println!(
            "  {} {} copies × {} = {} reclaimable",
            format!("[{}/{}]", i + 1, groups.len()).dimmed(),
            g.entries.len().to_string().cyan(),
            format_bytes(g.size),
            format_bytes(g.reclaimable()).yellow(),
        );

        let items: Vec<String> = g
            .entries
            .iter()
            .map(|e| {
                if e.paths.len() == 1 {
                    e.primary_path().display().to_string()
                } else {
                    format!(
                        "{}  (+{} hardlink{})",
                        e.primary_path().display(),
                        e.paths.len() - 1,
                        if e.paths.len() == 2 { "" } else { "s" }
                    )
                }
            })
            .collect();
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

        for (j, e) in g.entries.iter().enumerate() {
            if j == idx {
                continue;
            }
            for p in &e.paths {
                to_delete.push(p.clone());
            }
            delete_size += g.size;
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
        make_entry(path, bytes.len() as u64)
    }

    #[cfg(unix)]
    fn test_inode_key(path: &Path) -> Option<crate::walker::InodeKey> {
        use std::os::unix::fs::MetadataExt;
        let m = fs::symlink_metadata(path).ok()?;
        Some((m.dev(), m.ino()))
    }

    #[cfg(not(unix))]
    fn test_inode_key(_path: &Path) -> Option<crate::walker::InodeKey> {
        None
    }

    fn make_entry(path: &Path, size: u64) -> FileEntry {
        FileEntry {
            path: path.to_path_buf(),
            size,
            extension: path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("none")
                .to_lowercase(),
            modified: SystemTime::now(),
            inode: test_inode_key(path),
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
        assert_eq!(groups[0].entries.len(), 2);
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
        assert_eq!(groups[0].entries.len(), 3);
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
        assert_eq!(groups[0].entries.len(), 2);
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
            .entries
            .iter()
            .map(|e| {
                e.primary_path()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, vec!["a.bin", "m.bin", "z.bin"]);
    }

    #[cfg(unix)]
    #[test]
    fn hardlinks_collapse_into_one_entry() {
        let dir = tempdir().unwrap();
        let payload = vec![5u8; 4096];
        let a = write_file(&dir.path().join("a.bin"), &payload);
        fs::hard_link(dir.path().join("a.bin"), dir.path().join("a_link.bin")).unwrap();
        let a_link = make_entry(&dir.path().join("a_link.bin"), payload.len() as u64);
        let b = write_file(&dir.path().join("b.bin"), &payload);

        let groups = find(&[a, a_link, b], 0);
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].entries.len(),
            2,
            "hardlinks must collapse into a single entry"
        );
        assert_eq!(groups[0].reclaimable(), 4096);
        let hardlinked = groups[0]
            .entries
            .iter()
            .find(|e| e.paths.len() == 2)
            .expect("one entry should expose both hardlink paths");
        assert_eq!(hardlinked.paths.len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn pure_hardlinks_are_not_reported_as_dupes() {
        let dir = tempdir().unwrap();
        write_file(&dir.path().join("a.bin"), &vec![6u8; 4096]);
        fs::hard_link(dir.path().join("a.bin"), dir.path().join("a_link.bin")).unwrap();
        let a = make_entry(&dir.path().join("a.bin"), 4096);
        let a_link = make_entry(&dir.path().join("a_link.bin"), 4096);
        let groups = find(&[a, a_link], 0);
        assert!(
            groups.is_empty(),
            "a single inode with multiple hardlinks wastes no space"
        );
    }
}
