use std::sync::atomic::{AtomicBool, Ordering};

pub static IS_ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn is_english() -> bool {
    IS_ENGLISH.load(Ordering::Relaxed)
}

pub fn t<'a>(ja: &'a str, en: &'a str) -> &'a str {
    if is_english() {
        en
    } else {
        ja
    }
}

pub fn error_title() -> &'static str {
    t("エラー", "Error")
}

pub fn destination_title() -> &'static str {
    t("保存先を選択してください", "Select a destination")
}
