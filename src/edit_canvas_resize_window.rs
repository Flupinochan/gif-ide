use crate::gif_data::GifFile;
use crate::window::show_window;
use crate::{AppWindow, EditCanvasResizeWindow, LoadingState};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

// EditCanvasResizeWindowのfilter-type-indexと対応
const FILTER_TYPES: [image::imageops::FilterType; 5] = [
    image::imageops::FilterType::Nearest,
    image::imageops::FilterType::Triangle,
    image::imageops::FilterType::CatmullRom,
    image::imageops::FilterType::Gaussian,
    image::imageops::FilterType::Lanczos3,
];

pub(crate) fn register_callbacks(
    ui: &AppWindow,
    edit_canvas_resize_window: &EditCanvasResizeWindow,
    gif_file_ref: &Arc<Mutex<Option<GifFile>>>,
) {
    // 表示 Callback
    let ui_weak_edit_canvas_resize = ui.as_weak();
    let resize_window_weak_edit_canvas_resize = edit_canvas_resize_window.as_weak();
    ui.on_edit_canvas_resize(move || {
        let (Some(ui), Some(resize_window)) = (
            ui_weak_edit_canvas_resize.upgrade(),
            resize_window_weak_edit_canvas_resize.upgrade(),
        ) else {
            return;
        };

        let width = ui.get_gif_canvas_width();
        let height = ui.get_gif_canvas_height();
        resize_window.set_current_canvas_width(width);
        resize_window.set_current_canvas_height(height);
        resize_window.set_new_canvas_width(width);
        resize_window.set_new_canvas_height(height);
        resize_window.set_state(LoadingState::Form);

        show_window!(resize_window, ui);
    });

    // Cancel Callback
    let resize_window_weak_cancel = edit_canvas_resize_window.as_weak();
    edit_canvas_resize_window.on_cancel(move || {
        if let Some(w) = resize_window_weak_cancel.upgrade() {
            w.hide().unwrap();
        }
    });

    // Ok Callback
    let ui_weak_start_canvas_resize = ui.as_weak();
    let resize_window_weak_start_canvas_resize = edit_canvas_resize_window.as_weak();
    let gif_ref_resize = gif_file_ref.clone();
    edit_canvas_resize_window.on_start_canvas_resize(move || {
        let (Some(ui), Some(resize_window)) = (
            ui_weak_start_canvas_resize.upgrade(),
            resize_window_weak_start_canvas_resize.upgrade(),
        ) else {
            return;
        };

        let new_width = resize_window.get_new_canvas_width();
        let new_height = resize_window.get_new_canvas_height();
        let filter_type = FILTER_TYPES[resize_window.get_filter_type_index() as usize];

        let Some(mut gif) = gif_ref_resize.lock().unwrap().clone() else {
            return;
        };

        resize_window.set_state(LoadingState::Processing);

        let gif_ref_resize = gif_ref_resize.clone();
        let ui_weak = ui.as_weak();
        let resize_window_weak = resize_window.as_weak();
        tokio::task::spawn_blocking(move || {
            gif.resize_canvas(new_width as u16, new_height as u16, filter_type);

            let _ = slint::invoke_from_event_loop(move || {
                let (Some(ui), Some(resize_window)) =
                    (ui_weak.upgrade(), resize_window_weak.upgrade())
                else {
                    return;
                };

                let frame_data = gif.build_frame_data();
                ui.set_frames(ModelRc::from(Rc::new(VecModel::from(frame_data))));
                ui.set_gif_canvas_width(new_width);
                ui.set_gif_canvas_height(new_height);

                *gif_ref_resize.lock().unwrap() = Some(gif);

                resize_window.set_state(LoadingState::Success);
            });
        });
    });
}
