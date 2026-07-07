use crate::gif_data::{frame_data_from_buffers, Gif, GifFile};
use crate::window::show_window;
use crate::{AppWindow, EditFrameDropWindow, FramePreview, LoadingState};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

pub(crate) fn register_callbacks(
    ui: &AppWindow,
    edit_frame_drop_window: &EditFrameDropWindow,
    gif_file_ref: &Arc<Mutex<Option<GifFile>>>,
) {
    // フレーム間引きウィンドウ表示Callback
    let ui_weak_edit_frame_drop = ui.as_weak();
    let drop_window_weak_edit_frame_drop = edit_frame_drop_window.as_weak();
    ui.on_edit_frame_drop(move || {
        let (Some(ui), Some(drop_window)) = (
            ui_weak_edit_frame_drop.upgrade(),
            drop_window_weak_edit_frame_drop.upgrade(),
        ) else {
            return;
        };

        let frames = ui.get_frames();
        drop_window.set_current_total_frames(frames.row_count() as i32);

        let preview: Vec<FramePreview> = (0..frames.row_count())
            .filter_map(|i| frames.row_data(i))
            .map(|f| FramePreview { image: f.image })
            .collect();
        drop_window.set_frames(ModelRc::from(Rc::new(VecModel::from(preview))));
        drop_window.set_state(LoadingState::Form);

        show_window!(drop_window, ui);
    });

    // フレーム間引きウィンドウ Cancel Callback
    let drop_window_weak_cancel = edit_frame_drop_window.as_weak();
    edit_frame_drop_window.on_cancel(move || {
        if let Some(w) = drop_window_weak_cancel.upgrade() {
            w.hide().unwrap();
        }
    });

    // フレーム間引き実行Callback
    let ui_weak_start_frame_drop = ui.as_weak();
    let drop_window_weak_start_frame_drop = edit_frame_drop_window.as_weak();
    let gif_ref_drop = gif_file_ref.clone();
    edit_frame_drop_window.on_start_frame_drop(move || {
        let (Some(ui), Some(drop_window)) = (
            ui_weak_start_frame_drop.upgrade(),
            drop_window_weak_start_frame_drop.upgrade(),
        ) else {
            return;
        };

        let interval = drop_window.get_frame_drop_interval();
        let start_index = drop_window.get_frame_drop_start_index();

        // UIスレッドで take() と total 取得を原子的に行う (TOCTOU 排除)。
        // take() は O(1) のポインタ交換のみのため UIスレッドでも問題ない。
        let Some(mut gif) = gif_ref_drop.lock().unwrap().take() else {
            return;
        };
        let total = gif.frames().len();

        drop_window.set_state(LoadingState::Processing);
        drop_window.set_progress_current(0);
        drop_window.set_progress_total(total as i32);

        let gif_ref_drop = gif_ref_drop.clone();
        let ui_weak = ui.as_weak();
        let drop_window_weak = drop_window.as_weak();
        let drop_window_weak_progress = drop_window.as_weak();
        tokio::task::spawn_blocking(move || {
            let step = (total / 100).max(1);
            let mut last_reported = 0usize;
            gif.retain_frames(
                interval,
                start_index,
                Some(&mut |n: usize| {
                    if n != total && n - last_reported < step {
                        return;
                    }
                    last_reported = n;
                    let drop_window_weak_progress = drop_window_weak_progress.clone();
                    crate::logging::warn_on_err(
                        slint::invoke_from_event_loop(move || {
                            if let Some(w) = drop_window_weak_progress.upgrade() {
                                w.set_progress_current(n as i32);
                            }
                        }),
                        "invoke_from_event_loop failed (frame drop progress)",
                    );
                }),
            );

            // フレームタイムライン用バッファの合成も背景スレッドで完了させる (UIスレッドでは行わない)
            let buffers = gif.build_frame_buffers();

            // invoke_from_event_loop の前に gif を返却する。
            // これにより:
            // 1. invoke_from_event_loop が Err を返した場合 (イベントループ停止) でも gif が消失しない
            // 2. ウィンドウが閉じられた場合の早期 return でも gif が消失しない
            // 3. バッファ構築後にユーザーが delay を編集しても GifFile に書き込まれる
            *gif_ref_drop.lock().unwrap() = Some(gif);

            crate::logging::warn_on_err(
                slint::invoke_from_event_loop({
                    let gif_ref_drop = gif_ref_drop.clone();
                    move || {
                        let Some(gif) = gif_ref_drop.lock().unwrap().take() else {
                            return;
                        };
                        let (Some(ui), Some(drop_window)) =
                            (ui_weak.upgrade(), drop_window_weak.upgrade())
                        else {
                            // ウィンドウが閉じられた場合でも gif を復元する
                            *gif_ref_drop.lock().unwrap() = Some(gif);
                            return;
                        };

                        let frame_data = frame_data_from_buffers(buffers);
                        let new_len = frame_data.len() as i32;
                        ui.set_frames(ModelRc::from(Rc::new(VecModel::from(frame_data))));
                        if new_len > 0 {
                            ui.set_selected_frame_index(
                                ui.get_selected_frame_index().clamp(0, new_len - 1),
                            );
                        }
                        drop_window.set_current_total_frames(new_len);
                        *gif_ref_drop.lock().unwrap() = Some(gif);
                        drop_window.set_state(LoadingState::Success);
                    }
                }),
                "invoke_from_event_loop failed (frame drop result)",
            );
        });
    });
}
