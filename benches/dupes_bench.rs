use bigfiles::dupes;
use bigfiles::walker::FileEntry;
use criterion::{criterion_group, criterion_main, Criterion};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::SystemTime;
use tempfile::tempdir;

fn build_corpus(
    dir: &std::path::Path,
    n_groups: usize,
    copies: usize,
    size: usize,
) -> Vec<FileEntry> {
    let mut files = Vec::with_capacity(n_groups * copies);
    for g in 0..n_groups {
        let payload = vec![(g as u8).wrapping_mul(31); size];
        for c in 0..copies {
            let path: PathBuf = dir.join(format!("g{}_c{}.bin", g, c));
            let mut f = File::create(&path).unwrap();
            f.write_all(&payload).unwrap();
            files.push(FileEntry {
                path,
                size: size as u64,
                extension: "bin".to_string(),
                modified: SystemTime::now(),
                inode: None,
            });
        }
    }
    files
}

fn bench_dupes(c: &mut Criterion) {
    let dir = tempdir().unwrap();
    let files = build_corpus(dir.path(), 50, 4, 8 * 1024);

    c.bench_function("dupes_find_50x4_8kb", |b| {
        b.iter(|| dupes::find(&files, 0));
    });
}

criterion_group!(benches, bench_dupes);
criterion_main!(benches);
