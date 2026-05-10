use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

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

pub struct WalkOptions {
    pub skip_hidden: bool,
    pub max_depth: Option<usize>,
    pub respect_ignore: bool,
}

pub fn collect(root: &Path, opts: WalkOptions) -> ScanResult {
    let files: Mutex<Vec<FileEntry>> = Mutex::new(Vec::new());
    let skipped = AtomicUsize::new(0);

    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(opts.skip_hidden)
        .git_ignore(opts.respect_ignore)
        .git_global(opts.respect_ignore)
        .git_exclude(opts.respect_ignore)
        .ignore(opts.respect_ignore)
        .parents(opts.respect_ignore)
        .require_git(false);
    if let Some(d) = opts.max_depth {
        builder.max_depth(Some(d));
    }

    builder.build_parallel().run(|| {
        let files = &files;
        let skipped = &skipped;
        Box::new(move |result| {
            let entry = match result {
                Ok(e) => e,
                Err(_) => {
                    skipped.fetch_add(1, Ordering::Relaxed);
                    return ignore::WalkState::Continue;
                }
            };

            let ft = match entry.file_type() {
                Some(ft) => ft,
                None => return ignore::WalkState::Continue,
            };
            if !ft.is_file() {
                return ignore::WalkState::Continue;
            }

            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => {
                    skipped.fetch_add(1, Ordering::Relaxed);
                    return ignore::WalkState::Continue;
                }
            };

            let modified = match meta.modified() {
                Ok(t) => t,
                Err(_) => {
                    skipped.fetch_add(1, Ordering::Relaxed);
                    return ignore::WalkState::Continue;
                }
            };

            let path = entry.path().to_path_buf();
            let extension = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("none")
                .to_lowercase();

            let entry = FileEntry {
                path,
                size: meta.len(),
                extension,
                modified,
            };

            if let Ok(mut guard) = files.lock() {
                guard.push(entry);
            }
            ignore::WalkState::Continue
        })
    });

    ScanResult {
        files: files.into_inner().unwrap_or_default(),
        skipped: skipped.load(Ordering::Relaxed),
    }
}
