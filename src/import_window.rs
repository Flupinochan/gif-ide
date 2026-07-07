use crate::gif_data::{frame_data_from_buffers, GifFile};
use crate::window::{show_message_dialog, show_window};
use crate::{AppWindow, ImportFileWindow};
use rfd::AsyncFileDialog;
use slint::{ComponentHandle, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

async fn import_file() -> Option<PathBuf> {
    AsyncFileDialog::new()
        .add_filter("GIF", &["gif"])
        .set_title(crate::i18n::t("GIFを選択してください", "Select a GIF file"))
        .pick_file()
        .await
        .map(|handle| handle.path().to_path_buf())
}

async fn import_video_file() -> Option<PathBuf> {
    AsyncFileDialog::new()
        .add_filter(
            "Movie",
            &[
                "mp4", "m4v", "mov", "3gp", "3g2", "avi", "mkv", "webm", "wmv", "flv", "ogv",
            ],
        )
        .set_title(crate::i18n::t(
            "動画を選択してください",
            "Select a video file",
        ))
        .pick_file()
        .await
        .map(|handle| handle.path().to_path_buf())
}

// 読み込んだGifFileの内容をUIに反映
// frame_buffersは背景スレッドで合成済みのため、ここではImageへの軽量なラップのみ行う
fn apply_gif_file_to_ui(
    ui: &AppWindow,
    gif_file: &GifFile,
    filename: String,
    frame_buffers: Vec<(SharedPixelBuffer<Rgba8Pixel>, i32)>,
) {
    // フレームタイムライン更新
    let frames_model = Rc::new(VecModel::from(frame_data_from_buffers(frame_buffers)));
    ui.set_frames(ModelRc::from(frames_model));
    ui.set_selected_frame_index(0);
    ui.set_filename(SharedString::from(filename));
    ui.set_gif_canvas_width(gif_file.canvas_width as i32);
    ui.set_gif_canvas_height(gif_file.canvas_height as i32);
}

pub(crate) fn register_callbacks(
    ui: &AppWindow,
    import_window: &ImportFileWindow,
    gif_file_ref: &Arc<Mutex<Option<GifFile>>>,
) {
    // 読み込みウィンドウ表示Callback
    let ui_weak_import_file = ui.as_weak();
    let import_window_weak_import_file = import_window.as_weak();
    ui.on_import_file(move || {
        let (Some(ui), Some(import_window)) = (
            ui_weak_import_file.upgrade(),
            import_window_weak_import_file.upgrade(),
        ) else {
            return;
        };

        show_window!(import_window, ui);
    });

    // 読み込み元選択Callback
    let import_window_weak_select_import_path = import_window.as_weak();
    import_window.on_select_import_path(move || {
        let Some(import_window) = import_window_weak_select_import_path.upgrade() else {
            return;
        };

        let format_index = import_window.get_format_index();
        let import_window_weak_select_import_path = import_window.as_weak();
        tokio::spawn(async move {
            let path = if format_index == 0 {
                import_file().await
            } else {
                import_video_file().await
            };

            crate::logging::warn_on_err(
                slint::invoke_from_event_loop(move || {
                    let Some(import_window) = import_window_weak_select_import_path.upgrade()
                    else {
                        return;
                    };
                    if let Some(path) = path {
                        import_window.set_import_path(SharedString::from(
                            path.to_string_lossy().into_owned(),
                        ));
                    }
                }),
                "invoke_from_event_loop failed (select import path)",
            );
        });
    });

    // 読み込み実行Callback
    let ui_weak_start_import = ui.as_weak();
    let import_window_weak_start_import = import_window.as_weak();
    let gif_ref_import = gif_file_ref.clone();
    import_window.on_start_import(move || {
        let (Some(ui), Some(import_window)) = (
            ui_weak_start_import.upgrade(),
            import_window_weak_start_import.upgrade(),
        ) else {
            return;
        };

        let format_index = import_window.get_format_index();

        let path_buf = PathBuf::from(import_window.get_import_path().as_str());
        let filename = path_buf
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        import_window.hide().unwrap();
        ui.set_is_loading(true);

        let ui_weak_start_import = ui.as_weak();
        let gif_ref = gif_ref_import.clone();
        tokio::task::spawn_blocking(move || {
            let result = if format_index == 0 {
                GifFile::new(&path_buf)
            } else {
                GifFile::from_video(&path_buf)
            }
            // フレームタイムライン用バッファの合成も背景スレッドで完了させる (UIスレッドでは行わない)
            .map(|gif_file| {
                let buffers = gif_file.build_frame_buffers();
                (gif_file, buffers)
            });

            crate::logging::warn_on_err(
                slint::invoke_from_event_loop(move || {
                    let Some(ui) = ui_weak_start_import.upgrade() else {
                        return;
                    };
                    ui.set_is_loading(false);
                    match result {
                        Ok((gif_file, buffers)) => {
                            apply_gif_file_to_ui(&ui, &gif_file, filename, buffers);
                            *gif_ref.lock().unwrap() = Some(gif_file);
                        }
                        Err(e) => {
                            let label = crate::i18n::t(
                                if format_index == 0 { "GIF" } else { "動画" },
                                if format_index == 0 { "GIF" } else { "video" },
                            );
                            show_message_dialog(
                                crate::i18n::error_title(),
                                crate::i18n::t(
                                    &format!("{label}の読み込みに失敗しました: {e}"),
                                    &format!("Failed to load {label}: {e}"),
                                ),
                                &ui,
                            );
                        }
                    }
                }),
                "invoke_from_event_loop failed (import result)",
            );
        });
    });

    // 読み込みウィンドウCancel Callback
    let import_window_weak_cancel = import_window.as_weak();
    import_window.on_cancel(move || {
        if let Some(d) = import_window_weak_cancel.upgrade() {
            d.hide().unwrap();
        }
    });
}
