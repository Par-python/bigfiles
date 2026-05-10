use std::path::Path;
use std::time::SystemTime;
use walkdir::WalkDir;

pub struct FileEntry {
    pub size: u64,
    pub extension: String,
    pub modified: SystemTime,
}

pub struct ScanResult {
    pub files: Vec<FileEntry>,
    pub skipped: usize,
}

pub fn collect(root: &Path, skip_hidden: bool) -> ScanResult {
    let mut files = Vec::new();
    let mut skipped = 0usize;

    let walker = WalkDir::new(root).into_iter().filter_entry(move |e| {
        if !skip_hidden {
            return true;
        }
        if e.depth() == 0 {
            return true;
        }
        !e.file_name().to_string_lossy().starts_with('.')
    });

    for entry in walker {
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
            size: meta.len(),
            extension,
            modified,
        });
    }

    ScanResult { files, skipped }
}
