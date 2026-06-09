// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gif_data;

use anyhow::Result;
use rfd::FileDialog;
use slint::{Model, ModelRc, SharedString, Timer, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::gif_data::{Gif, GifFile};

slint::include_modules!();

fn show_dialog(title: &str, message: &str, parent: &AppWindow) {
    let dialog = MessageDialog::new().unwrap();
    dialog.global::<Palette>().set_color_scheme(parent.global::<Palette>().get_color_scheme());
    dialog.set_title_text(SharedString::from(title));
    dialog.set_message(SharedString::from(message));
    let dialog_weak = dialog.as_weak();
    dialog.on_ok_clicked(move || {
        if let Some(d) = dialog_weak.upgrade() {
            d.hide().unwrap();
        }
    });
    let parent_pos = parent.window().position();
    let parent_size = parent.window().size();
    let x = parent_pos.x + parent_size.width as i32 / 2;
    let y = parent_pos.y + parent_size.height as i32 / 2;
    dialog
        .window()
        .set_position(slint::PhysicalPosition::new(x, y));
    dialog.show().unwrap();
}

fn pick_gif_file() -> Option<PathBuf> {
    FileDialog::new()
        .add_filter("GIF", &["gif"])
        .set_directory("/")
        .set_title("GIFを選択してください")
        .pick_file()
}

fn save_gif_file() -> Option<PathBuf> {
    FileDialog::new()
        .add_filter("GIF", &["gif"])
        .set_title("保存先を選択してください")
        .save_file()
}

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
        let next = (frame_idx + 1) % frame_count;
        ui.set_selected_frame_index(next as i32);
        schedule_next_frame(ui_weak, next);
    });
}

fn main() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let _guard = rt.enter();

    let ui = AppWindow::new()?;

    let gif_file_ref: Rc<RefCell<Option<GifFile>>> = Rc::new(RefCell::new(None));

    // 画面最大化 (仕様が変わるかもしれないため消さないこと!!)
    // let ui_weak = ui.as_weak();
    // slint::invoke_from_event_loop(move || {
    //     if let Some(ui) = ui_weak.upgrade() {
    //         ui.window().set_maximized(true);
    //     }
    // })
    // .unwrap();

    // GIFファイル選択Callback
    let ui_weak = ui.as_weak();
    let gif_ref_open = gif_file_ref.clone();
    ui.on_pick_gif(move || {
        let Some(path_buf) = pick_gif_file() else {
            return;
        };
        let filename = path_buf
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        let Some(ui) = ui_weak.upgrade() else { return };
        ui.set_is_loading(true);
        let gif_ref = gif_ref_open.clone();
        slint::spawn_local(async move {
            let result = tokio::task::spawn_blocking(move || GifFile::new(&path_buf))
                .await
                .unwrap();
            ui.set_is_loading(false);
            match result {
                Ok(gif_file) => {
                    *gif_ref.borrow_mut() = Some(gif_file.clone());
                    // 再生時間更新
                    let total_duration_cs: u32 = gif_file
                        .frames()
                        .iter()
                        .map(|frame| frame.delay as u32)
                        .sum();
                    let total_seconds = total_duration_cs / 100;
                    let formatted = format!("{:02}:{:02}", total_seconds / 60, total_seconds % 60);
                    ui.set_total_duration(SharedString::from(formatted));

                    // フレームデータ構築
                    let frame_data: Vec<FrameData> = gif_file
                        .frames()
                        .iter()
                        .enumerate()
                        .filter_map(|(i, f)| {
                            gif_file.frame_image(i).map(|img| FrameData {
                                image: img,
                                delay: (f.delay as i32).max(2),
                            })
                        })
                        .collect();
                    let frames_model = Rc::new(VecModel::from(frame_data));
                    // フレームタイムライン更新
                    ui.set_frames(ModelRc::from(frames_model));
                    ui.set_selected_frame_index(0);
                    ui.set_current_filename(SharedString::from(filename));
                    ui.set_canvas_width(gif_file.canvas_width as i32);
                    ui.set_canvas_height(gif_file.canvas_height as i32);

                    // 再生Callback
                    let ui_weak_for_play = ui.as_weak();
                    ui.on_play(move |start_index| {
                        let Some(ui) = ui_weak_for_play.upgrade() else {
                            return;
                        };
                        if ui.get_is_play() {
                            schedule_next_frame(ui.as_weak(), start_index as usize);
                        }
                    });
                }
                Err(e) => {
                    show_dialog(
                        "エラー",
                        &format!("GIFの読み込みに失敗しました: {}", e),
                        &ui,
                    );
                }
            }
        })
        .unwrap();
    });

    // GIF出力Callback
    let ui_weak_export = ui.as_weak();
    let gif_ref_export = gif_file_ref.clone();
    ui.on_export_gif(move || {
        let Some(path) = save_gif_file() else { return };
        let gif_clone = gif_ref_export.borrow().clone();
        let Some(gif) = gif_clone else { return };
        let ui_weak = ui_weak_export.clone();
        slint::spawn_local(async move {
            let result = tokio::task::spawn_blocking(move || gif.export(&path))
                .await
                .unwrap();
            if let Err(e) = result {
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak.upgrade() {
                        show_dialog("エラー", &format!("GIFの出力に失敗しました: {}", e), &ui);
                    }
                });
            }
        })
        .unwrap();
    });

    // 画像出力Callback
    let ui_weak_image = ui.as_weak();
    ui.on_export_selected_image(move || {
        let Some(ui) = ui_weak_image.upgrade() else {
            return;
        };

        let dialog = ExportImageDialog::new().unwrap();
        dialog.global::<Palette>().set_color_scheme(ui.global::<Palette>().get_color_scheme());

        let dialog_weak = dialog.as_weak();
        dialog.on_ok_clicked(move || {
            if let Some(d) = dialog_weak.upgrade() {
                d.hide().unwrap();
            }
        });

        let dialog_weak = dialog.as_weak();
        dialog.on_cancel_clicked(move || {
            if let Some(d) = dialog_weak.upgrade() {
                d.hide().unwrap();
            }
        });

        let pos = ui.window().position();
        let size = ui.window().size();
        dialog.window().set_position(slint::PhysicalPosition::new(
            pos.x + size.width as i32 / 2,
            pos.y + size.height as i32 / 2,
        ));

        dialog.show().unwrap();
    });

    ui.run()?;

    Ok(())
}
