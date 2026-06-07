// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gif_data;

use anyhow::Result;
use rfd::FileDialog;
use slint::{Model, ModelRc, SharedString, VecModel};
use std::path::PathBuf;
use std::rc::Rc;

use crate::gif_data::{Gif, GifFile};

slint::include_modules!();

fn show_dialog(title: &str, message: &str, parent: &AppWindow) {
    let dialog = MessageDialog::new().unwrap();
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

fn main() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let _guard = rt.enter();

    let ui = AppWindow::new()?;
    let ui_weak = ui.as_weak();

    slint::invoke_from_event_loop(move || {
        if let Some(ui) = ui_weak.upgrade() {
            ui.window().set_maximized(true);
        }
    })
    .unwrap();

    let ui_weak = ui.as_weak();
    ui.on_pick_gif(move || {
        let Some(path_buf) = pick_gif_file() else {
            return;
        };

        let Some(ui) = ui_weak.upgrade() else { return };
        ui.set_is_loading(true);
        slint::spawn_local(async move {
            let result = tokio::task::spawn_blocking(move || GifFile::new(&path_buf))
                .await
                .unwrap();
            ui.set_is_loading(false);
            match result {
                Ok(gif_file) => {
                    ui.set_gif_frame_count(gif_file.frame_count() as i32);

                    if let Some(image) = gif_file.frame_image(0) {
                        ui.set_gif_image(image);
                        ui.set_selected_frame_index(0);
                    }

                    let frame_images: Vec<slint::Image> = (0..gif_file.frame_count())
                        .filter_map(|i| gif_file.frame_image(i))
                        .collect();
                    let frames_model = Rc::new(VecModel::from(frame_images));
                    let frames_model_ref = frames_model.clone();
                    ui.set_frames(ModelRc::from(frames_model));

                    let ui_weak_for_frame = ui.as_weak();
                    ui.on_frame_selected(move |index| {
                        if let Some(image) = frames_model_ref.row_data(index as usize) {
                            if let Some(ui) = ui_weak_for_frame.upgrade() {
                                ui.set_gif_image(image);
                            }
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

    ui.run()?;

    Ok(())
}
