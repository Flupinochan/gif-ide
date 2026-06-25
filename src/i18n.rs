use std::sync::atomic::{AtomicBool, Ordering};

pub static IS_ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn is_english() -> bool {
    IS_ENGLISH.load(Ordering::Relaxed)
}

pub fn t(ja: &'static str, en: &'static str) -> &'static str {
    if is_english() {
        en
    } else {
        ja
    }
}
