// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_window;
mod edit_canvas_resize_window;
mod edit_frame_drop_window;
mod export_window;
mod ffmpeg;
mod gif_data;
mod import_window;
mod window;

use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;

use crate::gif_data::GifFile;

slint::include_modules!();

fn main() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let _guard = rt.enter();

    let ui = AppWindow::new()?;
    let export_window = ExportFileWindow::new()?;
    let import_window = ImportFileWindow::new()?;
    let edit_frame_drop_window = EditFrameDropWindow::new()?;
    let edit_canvas_resize_window = EditCanvasResizeWindow::new()?;

    let gif_file_ref: Rc<RefCell<Option<GifFile>>> = Rc::new(RefCell::new(None));

    app_window::register_callbacks(&ui, &gif_file_ref);
    import_window::register_callbacks(&ui, &import_window, &gif_file_ref);
    export_window::register_callbacks(&ui, &export_window, &gif_file_ref);
    edit_frame_drop_window::register_callbacks(&ui, &edit_frame_drop_window, &gif_file_ref);
    edit_canvas_resize_window::register_callbacks(&ui, &edit_canvas_resize_window, &gif_file_ref);

    ui.run()?;

    Ok(())
}
