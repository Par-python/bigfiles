use crate::classifier::categorize;
use crate::walker::FileEntry;
use serde::Serialize;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

#[derive(Serialize)]
pub struct CategorySummary {
    pub category: String,
    pub total_size: u64,
    pub file_count: usize,
    pub stale_size: u64,
    pub stale_count: usize,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SortKey {
    Size,
    Count,
    StaleSize,
    StaleCount,
    Name,
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
    sort_summaries(&mut summaries, SortKey::Size, false);
    summaries
}

pub fn sort_summaries(summaries: &mut [CategorySummary], key: SortKey, reverse: bool) {
    match key {
        SortKey::Size => summaries.sort_by_key(|s| std::cmp::Reverse(s.total_size)),
        SortKey::Count => summaries.sort_by_key(|s| std::cmp::Reverse(s.file_count)),
        SortKey::StaleSize => summaries.sort_by_key(|s| std::cmp::Reverse(s.stale_size)),
        SortKey::StaleCount => summaries.sort_by_key(|s| std::cmp::Reverse(s.stale_count)),
        SortKey::Name => summaries.sort_by(|a, b| a.category.cmp(&b.category)),
    }
    if reverse {
        summaries.reverse();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(
        category: &str,
        total: u64,
        count: usize,
        stale_size: u64,
        stale_count: usize,
    ) -> CategorySummary {
        CategorySummary {
            category: category.to_string(),
            total_size: total,
            file_count: count,
            stale_size,
            stale_count,
        }
    }

    fn names(v: &[CategorySummary]) -> Vec<&str> {
        v.iter().map(|s| s.category.as_str()).collect()
    }

    #[test]
    fn sort_by_size_desc_by_default() {
        let mut v = vec![
            s("a", 100, 1, 0, 0),
            s("b", 300, 1, 0, 0),
            s("c", 200, 1, 0, 0),
        ];
        sort_summaries(&mut v, SortKey::Size, false);
        assert_eq!(names(&v), vec!["b", "c", "a"]);
    }

    #[test]
    fn sort_by_count() {
        let mut v = vec![s("a", 0, 5, 0, 0), s("b", 0, 20, 0, 0), s("c", 0, 10, 0, 0)];
        sort_summaries(&mut v, SortKey::Count, false);
        assert_eq!(names(&v), vec!["b", "c", "a"]);
    }

    #[test]
    fn sort_by_stale_size() {
        let mut v = vec![
            s("a", 0, 0, 50, 0),
            s("b", 0, 0, 200, 0),
            s("c", 0, 0, 100, 0),
        ];
        sort_summaries(&mut v, SortKey::StaleSize, false);
        assert_eq!(names(&v), vec!["b", "c", "a"]);
    }

    #[test]
    fn sort_by_stale_count() {
        let mut v = vec![s("a", 0, 0, 0, 2), s("b", 0, 0, 0, 7), s("c", 0, 0, 0, 4)];
        sort_summaries(&mut v, SortKey::StaleCount, false);
        assert_eq!(names(&v), vec!["b", "c", "a"]);
    }

    #[test]
    fn sort_by_name_is_alphabetical() {
        let mut v = vec![
            s("video", 0, 0, 0, 0),
            s("audio", 0, 0, 0, 0),
            s("images", 0, 0, 0, 0),
        ];
        sort_summaries(&mut v, SortKey::Name, false);
        assert_eq!(names(&v), vec!["audio", "images", "video"]);
    }

    #[test]
    fn reverse_flips_order() {
        let mut v = vec![
            s("a", 100, 1, 0, 0),
            s("b", 300, 1, 0, 0),
            s("c", 200, 1, 0, 0),
        ];
        sort_summaries(&mut v, SortKey::Size, true);
        assert_eq!(names(&v), vec!["a", "c", "b"]);
    }
}
