pub mod analyzer;
pub mod cache;
pub mod classifier;
pub mod dupes;
pub mod format;
pub mod renderer;
pub mod tui;
pub mod walker;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

pub static INTERRUPTED: OnceLock<Arc<AtomicBool>> = OnceLock::new();

pub fn interrupted() -> bool {
    INTERRUPTED
        .get()
        .map(|f| f.load(Ordering::SeqCst))
        .unwrap_or(false)
}
