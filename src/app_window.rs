use crate::gif_data::GifFile;
use crate::AppWindow;
use slint::{ComponentHandle, Model, Timer};
use std::sync::{Arc, Mutex};

// 再帰処理で再生機能を実装
fn schedule_next_frame(ui_weak: slint::Weak<AppWindow>, frame_idx: usize) {
    let Some(ui) = ui_weak.upgrade() else { return };
    let frames = ui.get_frames();
    let delay_ms = frames
        .row_data(frame_idx)
        .map(|f| f.delay.max(2) as u64 * 10)
        .unwrap_or(20);
    let frame_count = frames.row_count();

    Timer::single_shot(std::time::Duration::from_millis(delay_ms), move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        if !ui.get_is_play() {
            return;
        }

        if frame_idx + 1 >= frame_count {
            if ui.get_is_repeat() {
                ui.set_selected_frame_index(0);
                schedule_next_frame(ui_weak, 0);
            } else {
                ui.set_is_play(false);
            }
            return;
        }

        let next = frame_idx + 1;
        ui.set_selected_frame_index(next as i32);
        schedule_next_frame(ui_weak, next);
    });
}

pub(crate) fn register_callbacks(ui: &AppWindow, gif_file_ref: &Arc<Mutex<Option<GifFile>>>) {
    // 毎回ownershipをmove
    // move対象はブロックで使用している変数のみ
    // EventListener内の参照ではスコープの管理が難しいため、upgradeする (参照できる場合のみ処理する) 方法で対応
    let ui_weak_play = ui.as_weak();
    ui.on_play(move |start_index| {
        let Some(ui) = ui_weak_play.upgrade() else {
            return;
        };
        if ui.get_is_play() {
            let frame_count = ui.get_frames().row_count();
            let start_index = if frame_count > 0 && start_index as usize >= frame_count - 1 {
                ui.set_selected_frame_index(0);
                0
            } else {
                start_index as usize
            };
            schedule_next_frame(ui.as_weak(), start_index);
        }
    });

    // delay一括適用Callback
    let ui_weak_apply_delay_to_all = ui.as_weak();
    let gif_ref_bulk_delay = gif_file_ref.clone();
    ui.on_apply_delay_to_all(move |delay| {
        let Some(ui) = ui_weak_apply_delay_to_all.upgrade() else {
            return;
        };
        let frames = ui.get_frames();
        for i in 0..frames.row_count() {
            if let Some(mut frame) = frames.row_data(i) {
                frame.delay = delay;
                frames.set_row_data(i, frame);
            }
        }

        if let Some(gif) = gif_ref_bulk_delay.lock().unwrap().as_mut() {
            for raw_frame in gif.frames_mut() {
                raw_frame.delay = delay.clamp(0, u16::MAX as i32) as u16;
            }
        }
    });

    // delay個別編集Callback
    let ui_weak_frame_delay_changed = ui.as_weak();
    let gif_ref_delay = gif_file_ref.clone();
    ui.on_frame_delay_changed(move |index, delay| {
        let Some(ui) = ui_weak_frame_delay_changed.upgrade() else {
            return;
        };
        let frames = ui.get_frames();
        if let Some(mut frame) = frames.row_data(index as usize) {
            frame.delay = delay as i32;
            frames.set_row_data(index as usize, frame);
        }

        if let Some(gif) = gif_ref_delay.lock().unwrap().as_mut() {
            if let Some(raw_frame) = gif.frames_mut().get_mut(index as usize) {
                raw_frame.delay = delay.clamp(0.0, u16::MAX as f32) as u16;
            }
        }
    });

    // 言語切替Callback
    let ui_weak_switch_language = ui.as_weak();
    ui.on_switch_language(move || {
        let Some(ui) = ui_weak_switch_language.upgrade() else {
            return;
        };
        let next_lang = if ui.get_current_language() == "en" {
            "ja"
        } else {
            "en"
        };
        let _ = slint::select_bundled_translation(next_lang);
        crate::i18n::IS_ENGLISH.store(next_lang == "en", std::sync::atomic::Ordering::Relaxed);
        ui.set_current_language(next_lang.into());
    });
}
