use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use serde::Serialize;
use crate::walker::FileEntry;
use crate::classifier::categorize;

#[derive(Serialize)]
pub struct CategorySummary {
    pub category: String,
    pub total_size: u64,
    pub file_count: usize,
    pub stale_size: u64,
    pub stale_count: usize,
}

pub fn analyze(files: &[FileEntry], stale_years: u64) -> Vec<CategorySummary> {
    let mut map: HashMap<&'static str, CategorySummary> = HashMap::new();
    let now = SystemTime::now();
    let stale_threshold = Duration::from_secs(stale_years * 365 * 24 * 60 * 60);

    for file in files {
        let cat = categorize(&file.extension);
        let entry = map.entry(cat).or_insert_with(|| CategorySummary {
            category: cat.to_string(),
            total_size: 0,
            file_count: 0,
            stale_size: 0,
            stale_count: 0,
        });

        entry.total_size += file.size;
        entry.file_count += 1;

        if let Ok(age) = now.duration_since(file.modified) {
            if age > stale_threshold {
                entry.stale_size += file.size;
                entry.stale_count += 1;
            }
        }
    }

    let mut summaries: Vec<CategorySummary> = map.into_values().collect();
    summaries.sort_by(|a, b| b.total_size.cmp(&a.total_size));
    summaries
}
