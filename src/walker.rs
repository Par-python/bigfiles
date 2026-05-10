use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
    pub extension: String,
    pub modified: SystemTime,
}

pub struct ScanResult {
    pub files: Vec<FileEntry>,
    pub skipped: usize,
}

pub fn collect(root: &Path, skip_hidden: bool, max_depth: Option<usize>) -> ScanResult {
    let mut files = Vec::new();
    let mut skipped = 0usize;

    let mut walker = WalkDir::new(root);
    if let Some(d) = max_depth {
        walker = walker.max_depth(d);
    }

    let iter = walker.into_iter().filter_entry(move |e| {
        if !skip_hidden {
            return true;
        }
        if e.depth() == 0 {
            return true;
        }
        !e.file_name().to_string_lossy().starts_with('.')
    });

    for entry in iter {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        let modified = match meta.modified() {
            Ok(t) => t,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        let extension = entry
            .path()
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("none")
            .to_lowercase();

        files.push(FileEntry {
            path: entry.path().to_path_buf(),
            size: meta.len(),
            extension,
            modified,
        });
    }

    ScanResult { files, skipped }
}
