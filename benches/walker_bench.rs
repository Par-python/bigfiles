use criterion::{criterion_group, criterion_main, Criterion};
use ignore::WalkBuilder;
use jwalk::WalkDir as JWalkDir;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;

fn build_shallow_wide(root: &Path, files: usize) {
    for i in 0..files {
        let mut f = File::create(root.join(format!("f{}.bin", i))).unwrap();
        f.write_all(b"x").unwrap();
    }
}

fn build_deep_narrow(root: &Path, depth: usize) {
    let mut cur = root.to_path_buf();
    for i in 0..depth {
        cur = cur.join(format!("d{}", i));
        fs::create_dir(&cur).unwrap();
        let mut f = File::create(cur.join("file.bin")).unwrap();
        f.write_all(b"x").unwrap();
    }
}

fn build_realistic(root: &Path) {
    fs::write(root.join(".gitignore"), "node_modules/\ntarget/\n").unwrap();
    let nm = root.join("node_modules");
    fs::create_dir(&nm).unwrap();
    for pkg in 0..50 {
        let pkg_dir = nm.join(format!("pkg{}", pkg));
        fs::create_dir(&pkg_dir).unwrap();
        for sub in 0..5 {
            let sub_dir = pkg_dir.join(format!("sub{}", sub));
            fs::create_dir(&sub_dir).unwrap();
            for i in 0..20 {
                let mut f = File::create(sub_dir.join(format!("f{}.js", i))).unwrap();
                f.write_all(b"// placeholder").unwrap();
            }
        }
    }
    let src = root.join("src");
    fs::create_dir(&src).unwrap();
    for i in 0..100 {
        let mut f = File::create(src.join(format!("m{}.rs", i))).unwrap();
        f.write_all(b"// placeholder").unwrap();
    }
}

fn count_ignore(root: &Path, respect_gitignore: bool) -> usize {
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .git_ignore(respect_gitignore)
        .git_global(respect_gitignore)
        .git_exclude(respect_gitignore)
        .ignore(respect_gitignore)
        .parents(respect_gitignore)
        .require_git(false)
        .follow_links(false);
    let mut n = 0;
    for entry in builder.build().flatten() {
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            n += 1;
        }
    }
    n
}

fn count_jwalk(root: &Path) -> usize {
    JWalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .count()
}

fn bench_walkers(c: &mut Criterion) {
    let shallow = TempDir::new().unwrap();
    build_shallow_wide(shallow.path(), 10_000);

    let deep = TempDir::new().unwrap();
    build_deep_narrow(deep.path(), 50);

    let real = TempDir::new().unwrap();
    build_realistic(real.path());

    let mut group = c.benchmark_group("walker");

    group.bench_function("shallow_wide_ignore_on", |b| {
        b.iter(|| count_ignore(shallow.path(), true))
    });
    group.bench_function("shallow_wide_ignore_off", |b| {
        b.iter(|| count_ignore(shallow.path(), false))
    });
    group.bench_function("shallow_wide_jwalk", |b| {
        b.iter(|| count_jwalk(shallow.path()))
    });

    group.bench_function("deep_narrow_ignore_on", |b| {
        b.iter(|| count_ignore(deep.path(), true))
    });
    group.bench_function("deep_narrow_ignore_off", |b| {
        b.iter(|| count_ignore(deep.path(), false))
    });
    group.bench_function("deep_narrow_jwalk", |b| b.iter(|| count_jwalk(deep.path())));

    group.bench_function("realistic_ignore_on", |b| {
        b.iter(|| count_ignore(real.path(), true))
    });
    group.bench_function("realistic_ignore_off", |b| {
        b.iter(|| count_ignore(real.path(), false))
    });
    group.bench_function("realistic_jwalk", |b| b.iter(|| count_jwalk(real.path())));

    group.finish();
}

criterion_group!(benches, bench_walkers);
criterion_main!(benches);
