use crate::gif_data::GifFile;
use crate::window::show_window;
use crate::{AppWindow, EditFrameDropWindow, FramePreview};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) fn register_callbacks(
    ui: &AppWindow,
    edit_frame_drop_window: &EditFrameDropWindow,
    gif_file_ref: &Rc<RefCell<Option<GifFile>>>,
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

        let mut gif_ref = gif_ref_drop.borrow_mut();
        let Some(gif) = gif_ref.as_mut() else {
            return;
        };

        gif.retain_frames(interval, start_index);

        let frame_data = gif.build_frame_data();
        let new_len = frame_data.len() as i32;
        ui.set_frames(ModelRc::from(Rc::new(VecModel::from(frame_data))));
        ui.set_selected_frame_index(ui.get_selected_frame_index().clamp(0, new_len - 1));

        drop_window.hide().unwrap();
    });
}
