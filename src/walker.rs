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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;

    fn write_file(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        f.write_all(bytes).unwrap();
    }

    fn default_opts() -> WalkOptions {
        WalkOptions {
            skip_hidden: false,
            max_depth: None,
            respect_ignore: false,
        }
    }

    fn names(result: &ScanResult) -> Vec<String> {
        let mut v: Vec<String> = result
            .files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn collects_regular_files_with_sizes_and_extensions() {
        let dir = tempdir().unwrap();
        write_file(&dir.path().join("a.txt"), b"hello");
        write_file(&dir.path().join("sub/b.RS"), b"fn main() {}");

        let r = collect(dir.path(), default_opts());
        assert_eq!(r.files.len(), 2);

        let a = r.files.iter().find(|f| f.path.ends_with("a.txt")).unwrap();
        assert_eq!(a.size, 5);
        assert_eq!(a.extension, "txt");

        let b = r.files.iter().find(|f| f.path.ends_with("b.RS")).unwrap();
        assert_eq!(b.extension, "rs", "extension should be lowercased");
    }

    #[test]
    fn files_without_extension_get_none() {
        let dir = tempdir().unwrap();
        write_file(&dir.path().join("Makefile"), b"x");
        let r = collect(dir.path(), default_opts());
        assert_eq!(r.files[0].extension, "none");
    }

    #[test]
    fn skip_hidden_excludes_dotfiles() {
        let dir = tempdir().unwrap();
        write_file(&dir.path().join("visible.txt"), b"x");
        write_file(&dir.path().join(".hidden"), b"x");

        let mut opts = default_opts();
        opts.skip_hidden = true;
        let r = collect(dir.path(), opts);
        assert_eq!(names(&r), vec!["visible.txt"]);
    }

    #[test]
    fn max_depth_one_only_scans_root_files() {
        let dir = tempdir().unwrap();
        write_file(&dir.path().join("root.txt"), b"x");
        write_file(&dir.path().join("sub/deep.txt"), b"x");

        let mut opts = default_opts();
        opts.max_depth = Some(1);
        let r = collect(dir.path(), opts);
        assert_eq!(names(&r), vec!["root.txt"]);
    }

    #[test]
    fn respects_gitignore_when_enabled() {
        let dir = tempdir().unwrap();
        write_file(&dir.path().join(".gitignore"), b"ignored.txt\n");
        write_file(&dir.path().join("kept.txt"), b"x");
        write_file(&dir.path().join("ignored.txt"), b"x");

        let mut opts = default_opts();
        opts.respect_ignore = true;
        let r = collect(dir.path(), opts);
        let n = names(&r);
        assert!(n.contains(&"kept.txt".to_string()));
        assert!(!n.contains(&"ignored.txt".to_string()));
    }

    #[test]
    fn ignores_gitignore_when_disabled() {
        let dir = tempdir().unwrap();
        write_file(&dir.path().join(".gitignore"), b"ignored.txt\n");
        write_file(&dir.path().join("ignored.txt"), b"x");

        let r = collect(dir.path(), default_opts());
        let n = names(&r);
        assert!(n.contains(&"ignored.txt".to_string()));
    }

    #[test]
    fn empty_dir_returns_no_files() {
        let dir = tempdir().unwrap();
        let r = collect(dir.path(), default_opts());
        assert!(r.files.is_empty());
        assert_eq!(r.skipped, 0);
    }

    #[test]
    fn excludes_directories_from_results() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("emptydir")).unwrap();
        write_file(&dir.path().join("f.txt"), b"x");
        let r = collect(dir.path(), default_opts());
        assert_eq!(r.files.len(), 1);
    }
}
