// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gif_data;

use anyhow::Result;
use rfd::AsyncFileDialog;
use slint::{Model, ModelRc, SharedString, Timer, VecModel};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::gif_data::{Gif, GifFile};

slint::include_modules!();

// window/dialog表示用macro
// windowとdialogで型は異なるが使用方法は同じためmacroで定義
// 1. set theme
// 2. centralize window
// 3. focus window
macro_rules! show_window {
    ($window:expr, $parent:expr) => {{
        $window
            .global::<Palette>()
            .set_color_scheme($parent.global::<Palette>().get_color_scheme());

        let parent_pos = $parent.window().position();
        let parent_size = $parent.window().size();
        $window.window().set_position(slint::PhysicalPosition::new(
            parent_pos.x + parent_size.width as i32 / 2,
            parent_pos.y + parent_size.height as i32 / 2,
        ));

        $window.show().unwrap();
        focus_window($window.window());
    }};
}

fn show_message_dialog(title: &str, message: &str, parent: &AppWindow) {
    let dialog = MessageDialog::new().unwrap();
    dialog.set_title_text(SharedString::from(title));
    dialog.set_message(SharedString::from(message));
    let dialog_weak = dialog.as_weak();
    dialog.on_close(move || {
        if let Some(d) = dialog_weak.upgrade() {
            d.hide().unwrap();
        }
    });
    show_window!(dialog, parent);
}

async fn import_file() -> Option<PathBuf> {
    AsyncFileDialog::new()
        .add_filter("GIF", &["gif"])
        .set_title("GIFを選択してください")
        .pick_file()
        .await
        .map(|handle| handle.path().to_path_buf())
}

async fn save_gif_file() -> Option<PathBuf> {
    AsyncFileDialog::new()
        .add_filter("GIF", &["gif"])
        .set_title("保存先を選択してください")
        .save_file()
        .await
        .map(|handle| handle.path().to_path_buf())
}

// ExportFileWindowのformat-indexと対応 (表示名, 拡張子, image::ImageFormat)
const IMAGE_FORMATS: [(&str, &str, image::ImageFormat); 6] = [
    ("PNG", "png", image::ImageFormat::Png),
    ("JPEG", "jpeg", image::ImageFormat::Jpeg),
    ("WEBP", "webp", image::ImageFormat::WebP),
    ("BMP", "bmp", image::ImageFormat::Bmp),
    ("ICO", "ico", image::ImageFormat::Ico),
    ("AVIF", "avif", image::ImageFormat::Avif),
];

// JPEGはアルファチャンネル非対応のため、透過部分を白背景に合成してRGBに変換する
// TODO: 背景色は固定で白としているが、将来的にユーザーが選択できるようにする
fn rgba_to_rgb_with_white_background(image: image::RgbaImage) -> image::RgbImage {
    let (width, height) = image.dimensions();
    let mut rgb = image::RgbImage::new(width, height);

    for (src, dst) in image.pixels().zip(rgb.pixels_mut()) {
        let alpha = src[3] as f32 / 255.0;
        for ch in 0..3 {
            dst[ch] = (src[ch] as f32 * alpha + 255.0 * (1.0 - alpha)).round() as u8;
        }
    }

    rgb
}

// 1フレーム分のバッファを指定形式でファイルに書き出す
fn encode_image_buffer(
    buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>,
    format: image::ImageFormat,
    is_jpeg: bool,
    quality: u8,
    path: &Path,
) -> image::ImageResult<()> {
    if is_jpeg {
        let rgba =
            image::RgbaImage::from_raw(buffer.width(), buffer.height(), buffer.as_bytes().to_vec())
                .expect("invalid image buffer");
        let rgb = rgba_to_rgb_with_white_background(rgba);
        let writer = std::io::BufWriter::new(std::fs::File::create(path)?);
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(writer, quality);
        encoder.encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
    } else {
        image::save_buffer_with_format(
            path,
            buffer.as_bytes(),
            buffer.width(),
            buffer.height(),
            image::ColorType::Rgba8,
            format,
        )
    }
}

// 全フレーム出力時に、連番を挿入したファイルパスを生成
fn build_indexed_path(base_path: &Path, index: usize, total: usize) -> PathBuf {
    // ファイル名_00.png のように0始まりなので最大インデックスは total - 1 して計算
    let width = (total - 1).to_string().len();
    let stem = base_path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = base_path.extension().unwrap_or_default().to_string_lossy();
    let dir = base_path.parent().unwrap_or_else(|| Path::new(""));
    dir.join(format!("{stem}_{index:0width$}.{ext}"))
}

// next_indexが指すフレームのエンコードタスクをJoinSetに1件投入し、インデックスを進める
fn spawn_next_encode_task(
    set: &mut tokio::task::JoinSet<image::ImageResult<()>>,
    next_index: &mut usize,
    buffers: &[slint::SharedPixelBuffer<slint::Rgba8Pixel>],
    path: &Path,
    format: image::ImageFormat,
    is_jpeg: bool,
    quality: u8,
) {
    let total = buffers.len();
    if *next_index >= total {
        return;
    }
    let index = *next_index;
    *next_index += 1;
    let buffer = buffers[index].clone();
    let target_path = if total == 1 {
        path.to_path_buf()
    } else {
        build_indexed_path(path, index, total)
    };
    set.spawn_blocking(move || {
        encode_image_buffer(&buffer, format, is_jpeg, quality, &target_path)
    });
}

async fn save_image_file(format_index: i32) -> Option<PathBuf> {
    let (name, ext, _) = IMAGE_FORMATS[format_index as usize];
    AsyncFileDialog::new()
        .add_filter(name, &[ext])
        .set_title("保存先を選択してください")
        .save_file()
        .await
        .map(|handle| handle.path().to_path_buf())
}

// すでにwindowが表示中の場合は2重表示せず最前面に表示
#[cfg(windows)]
fn focus_window(window: &slint::Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    let slint_handle = window.window_handle();
    let Ok(handle) = slint_handle.window_handle() else {
        return;
    };
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        let hwnd = handle.hwnd.get() as *mut std::ffi::c_void;
        unsafe {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
fn focus_window(_window: &slint::Window) {}

// ファイル出力先Explorerを表示
#[cfg(windows)]
fn open_in_explorer(path: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    let _ = Command::new("explorer")
        .raw_arg(format!("/select,\"{}\"", path.display()))
        .spawn();
}

#[cfg(not(windows))]
fn open_in_explorer(_path: &std::path::Path) {}

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

fn main() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let _guard = rt.enter();

    let ui = AppWindow::new()?;
    let export_window = ExportFileWindow::new()?;
    let import_window = ImportFileWindow::new()?;

    let gif_file_ref: Rc<RefCell<Option<GifFile>>> = Rc::new(RefCell::new(None));

    // 毎回ownershipをmove
    // move対象はブロックで使用している変数のみ
    // EventListener内の参照ではスコープの管理が難しいため、upgradeする (参照できる場合のみ処理する) 方法で対応
    let ui_weak_for_play = ui.as_weak();
    ui.on_play(move |start_index| {
        let Some(ui) = ui_weak_for_play.upgrade() else {
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
    let ui_weak_for_bulk_delay = ui.as_weak();
    ui.on_apply_delay_to_all(move |delay| {
        let Some(ui) = ui_weak_for_bulk_delay.upgrade() else {
            return;
        };
        let frames = ui.get_frames();
        for i in 0..frames.row_count() {
            if let Some(mut frame) = frames.row_data(i) {
                frame.delay = delay;
                frames.set_row_data(i, frame);
            }
        }
    });

    // 読み込みウィンドウ表示Callback
    let ui_weak_import = ui.as_weak();
    let import_window_weak_show = import_window.as_weak();
    ui.on_import_file(move || {
        let (Some(ui), Some(import_window)) =
            (ui_weak_import.upgrade(), import_window_weak_show.upgrade())
        else {
            return;
        };

        show_window!(import_window, ui);
    });

    // 読み込み元選択Callback
    let import_window_weak_pick = import_window.as_weak();
    import_window.on_select_import_path(move || {
        let Some(import_window) = import_window_weak_pick.upgrade() else {
            return;
        };

        let format_index = import_window.get_format_index();
        let import_window_weak = import_window.as_weak();
        slint::spawn_local(async move {
            let path = if format_index == 0 {
                import_file().await
            } else {
                // TODO: MP4インポート用のファイルピッカーを実装する (format_index == 1)
                None
            };

            let _ = slint::invoke_from_event_loop(move || {
                let Some(import_window) = import_window_weak.upgrade() else {
                    return;
                };
                if let Some(path) = path {
                    import_window
                        .set_import_path(SharedString::from(path.to_string_lossy().into_owned()));
                }
            });
        })
        .unwrap();
    });

    // 読み込み実行Callback
    let ui_weak_start = ui.as_weak();
    let import_window_weak_start = import_window.as_weak();
    let gif_ref_import = gif_file_ref.clone();
    import_window.on_start_import(move || {
        let (Some(ui), Some(import_window)) =
            (ui_weak_start.upgrade(), import_window_weak_start.upgrade())
        else {
            return;
        };

        let format_index = import_window.get_format_index();
        if format_index != 0 {
            // TODO: MP4の読み込み処理を実装する (format_index == 1)
            return;
        }

        let path_buf = PathBuf::from(import_window.get_import_path().as_str());
        let filename = path_buf
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        import_window.hide().unwrap();
        ui.set_is_loading(true);

        let ui_weak = ui.as_weak();
        let gif_ref = gif_ref_import.clone();
        slint::spawn_local(async move {
            let result = tokio::task::spawn_blocking(move || GifFile::new(&path_buf))
                .await
                .unwrap();

            let Some(ui) = ui_weak.upgrade() else { return };
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
                    ui.set_filename(SharedString::from(filename));
                    ui.set_gif_canvas_width(gif_file.canvas_width as i32);
                    ui.set_gif_canvas_height(gif_file.canvas_height as i32);
                }
                Err(e) => {
                    show_message_dialog(
                        "エラー",
                        &format!("GIFの読み込みに失敗しました: {}", e),
                        &ui,
                    );
                }
            }
        })
        .unwrap();
    });

    // 読み込みウィンドウCancel Callback
    let import_window_weak_cancel = import_window.as_weak();
    import_window.on_cancel(move || {
        if let Some(d) = import_window_weak_cancel.upgrade() {
            d.hide().unwrap();
        }
    });

    // 出力ウィンドウCallback
    let ui_weak_ok = ui.as_weak();
    let export_window_weak_ok = export_window.as_weak();
    let gif_ref_ok = gif_file_ref.clone();
    export_window.on_start_export(move || {
        let (Some(export_window), Some(ui)) =
            (export_window_weak_ok.upgrade(), ui_weak_ok.upgrade())
        else {
            return;
        };

        let path = PathBuf::from(export_window.get_export_path().as_str());
        let export_window_weak = export_window.as_weak();
        let ui_weak = ui.as_weak();

        enum ExportJob {
            Gif {
                gif: GifFile,
                loop_forever: bool,
                delays: Vec<u16>,
            },
            Image {
                buffers: Vec<slint::SharedPixelBuffer<slint::Rgba8Pixel>>,
                format: image::ImageFormat,
                is_jpeg: bool,
                quality: u8,
            },
        }

        let job = if export_window.get_is_gif() {
            let Some(gif) = gif_ref_ok.borrow().clone() else {
                return;
            };
            let frames = ui.get_frames();
            let delays: Vec<u16> = (0..frames.row_count())
                .filter_map(|i| frames.row_data(i))
                .map(|f| f.delay.clamp(0, u16::MAX as i32) as u16)
                .collect();
            ExportJob::Gif {
                gif,
                loop_forever: export_window.get_gif_loop_forever(),
                delays,
            }
        } else {
            let frames = ui.get_frames();
            let buffers: Vec<_> = if export_window.get_range_index() == 0 {
                let frame_index = ui.get_selected_frame_index() as usize;
                let Some(frame) = frames.row_data(frame_index) else {
                    return;
                };
                let Some(buffer) = frame.image.to_rgba8() else {
                    return;
                };
                vec![buffer]
            } else {
                (0..frames.row_count())
                    .filter_map(|i| frames.row_data(i)?.image.to_rgba8())
                    .collect()
            };

            let (_, _, format) = IMAGE_FORMATS[export_window.get_format_index() as usize - 1];

            // TODO: ICOは幅・高さとも1..=256の制約があるため、imageops::resizeで
            // アスペクト比を保ったまま縮小し、ユーザーにサイズ(256/128/64/32など)を選択させる必要がある
            ExportJob::Image {
                buffers,
                format,
                is_jpeg: export_window.get_is_jpeg(),
                quality: export_window.get_quality() as u8,
            }
        };

        export_window.set_state(ExportState::Processing);

        match job {
            ExportJob::Gif {
                gif,
                loop_forever,
                delays,
            } => {
                slint::spawn_local(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        gif.export(&path, loop_forever, &delays)
                    })
                    .await
                    .unwrap();

                    let _ = slint::invoke_from_event_loop(move || match result {
                        Ok(()) => {
                            if let Some(export_window) = export_window_weak.upgrade() {
                                export_window.set_state(ExportState::Success);
                            }
                        }
                        Err(e) => {
                            if let Some(ui) = ui_weak.upgrade() {
                                show_message_dialog(
                                    "エラー",
                                    &format!("GIFの出力に失敗しました: {}", e),
                                    &ui,
                                );
                            }
                        }
                    });
                })
                .unwrap();
            }
            ExportJob::Image {
                buffers,
                format,
                is_jpeg,
                quality,
            } => {
                slint::spawn_local(async move {
                    let total = buffers.len();
                    let parallelism = std::thread::available_parallelism()
                        .map(|n| n.get())
                        .unwrap_or(1);

                    let mut set = tokio::task::JoinSet::new();
                    let mut next_index = 0;

                    for _ in 0..parallelism.min(total) {
                        spawn_next_encode_task(
                            &mut set,
                            &mut next_index,
                            &buffers,
                            &path,
                            format,
                            is_jpeg,
                            quality,
                        );
                    }

                    let mut result: image::ImageResult<()> = Ok(());
                    while let Some(join_result) = set.join_next().await {
                        match join_result.unwrap() {
                            Ok(()) => spawn_next_encode_task(
                                &mut set,
                                &mut next_index,
                                &buffers,
                                &path,
                                format,
                                is_jpeg,
                                quality,
                            ),
                            Err(e) => {
                                if result.is_ok() {
                                    result = Err(e);
                                }
                            }
                        }
                    }

                    let _ = slint::invoke_from_event_loop(move || match result {
                        Ok(()) => {
                            if let Some(export_window) = export_window_weak.upgrade() {
                                export_window.set_state(ExportState::Success);
                            }
                        }
                        Err(e) => {
                            if let Some(ui) = ui_weak.upgrade() {
                                show_message_dialog(
                                    "エラー",
                                    &format!("画像の出力に失敗しました: {}", e),
                                    &ui,
                                );
                            }
                        }
                    });
                })
                .unwrap();
            }
        }
    });

    let export_window_weak_cancel = export_window.as_weak();
    export_window.on_cancel(move || {
        if let Some(d) = export_window_weak_cancel.upgrade() {
            d.hide().unwrap();
        }
    });

    let export_window_weak_open = export_window.as_weak();
    export_window.on_open_export_folder(move || {
        let Some(export_window) = export_window_weak_open.upgrade() else {
            return;
        };
        let path = PathBuf::from(export_window.get_export_path().as_str());
        open_in_explorer(&path);
    });

    let export_window_weak_pick = export_window.as_weak();
    export_window.on_select_export_path(move || {
        let Some(export_window) = export_window_weak_pick.upgrade() else {
            return;
        };

        let format_index = export_window.get_format_index();
        let is_gif = export_window.get_is_gif();
        let export_window_weak = export_window.as_weak();
        slint::spawn_local(async move {
            let path = if is_gif {
                save_gif_file().await
            } else {
                save_image_file(format_index - 1).await
            };

            let _ = slint::invoke_from_event_loop(move || {
                let Some(export_window) = export_window_weak.upgrade() else {
                    return;
                };
                if let Some(path) = path {
                    export_window
                        .set_export_path(SharedString::from(path.to_string_lossy().into_owned()));
                }
            });
        })
        .unwrap();
    });

    // 画像出力Callback
    let ui_weak_image = ui.as_weak();
    let export_window_weak_show = export_window.as_weak();
    ui.on_export_file(move || {
        let (Some(ui), Some(export_window)) =
            (ui_weak_image.upgrade(), export_window_weak_show.upgrade())
        else {
            return;
        };

        export_window.set_state(ExportState::Form);
        show_window!(export_window, ui);
    });

    ui.run()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    // テスト用GIFから全フレームをRGBAバッファとして読み込む
    // GIFファイルパスは環境変数 TEST_GIF_PATH で指定する
    fn load_test_buffers() -> Vec<slint::SharedPixelBuffer<slint::Rgba8Pixel>> {
        let path = std::env::var("TEST_GIF_PATH")
            .expect("環境変数 TEST_GIF_PATH にテスト用GIFファイルのパスを指定してください");
        let gif_file = GifFile::new(Path::new(&path)).expect("GIFファイルの読み込みに失敗");
        (0..gif_file.frames().len())
            .map(|i| {
                gif_file
                    .frame_image(i)
                    .and_then(|image| image.to_rgba8())
                    .expect("フレームの取得に失敗")
            })
            .collect()
    }

    // 1. 全フレームを1枚ずつ逐次エンコード (並行化なし)
    fn export_sequential(
        buffers: &[slint::SharedPixelBuffer<slint::Rgba8Pixel>],
        base_path: &Path,
    ) -> Duration {
        let total = buffers.len();
        let start = Instant::now();
        for (index, buffer) in buffers.iter().enumerate() {
            let target_path = build_indexed_path(base_path, index, total);
            encode_image_buffer(buffer, image::ImageFormat::Png, false, 100, &target_path).unwrap();
        }
        start.elapsed()
    }

    // 2. 全フレームを一度に並行エンコード (同時実行数の制限なし)
    fn export_full_parallel(
        rt: &tokio::runtime::Runtime,
        buffers: &[slint::SharedPixelBuffer<slint::Rgba8Pixel>],
        base_path: &Path,
    ) -> Duration {
        let total = buffers.len();
        let start = Instant::now();
        rt.block_on(async {
            let tasks: Vec<_> = buffers
                .iter()
                .enumerate()
                .map(|(index, buffer)| {
                    let buffer = buffer.clone();
                    let target_path = build_indexed_path(base_path, index, total);
                    tokio::task::spawn_blocking(move || {
                        encode_image_buffer(
                            &buffer,
                            image::ImageFormat::Png,
                            false,
                            100,
                            &target_path,
                        )
                    })
                })
                .collect();
            for task in tasks {
                task.await.unwrap().unwrap();
            }
        });
        start.elapsed()
    }

    // 3. 現在の実装: CPU論理コア数ごとにチャンク分割して並行エンコード
    fn export_chunked(
        rt: &tokio::runtime::Runtime,
        buffers: &[slint::SharedPixelBuffer<slint::Rgba8Pixel>],
        base_path: &Path,
        parallelism: usize,
    ) -> Duration {
        let total = buffers.len();
        let start = Instant::now();
        rt.block_on(async {
            for chunk_start in (0..total).step_by(parallelism) {
                let chunk_end = (chunk_start + parallelism).min(total);
                let tasks: Vec<_> = (chunk_start..chunk_end)
                    .map(|index| {
                        let buffer = buffers[index].clone();
                        let target_path = build_indexed_path(base_path, index, total);
                        tokio::task::spawn_blocking(move || {
                            encode_image_buffer(
                                &buffer,
                                image::ImageFormat::Png,
                                false,
                                100,
                                &target_path,
                            )
                        })
                    })
                    .collect();
                for task in tasks {
                    task.await.unwrap().unwrap();
                }
            }
        });
        start.elapsed()
    }

    // 4. 現在の実装: 1タスク完了ごとに次の1タスクを投入するストリーミング並行エンコード
    fn export_streaming(
        rt: &tokio::runtime::Runtime,
        buffers: &[slint::SharedPixelBuffer<slint::Rgba8Pixel>],
        base_path: &Path,
        parallelism: usize,
    ) -> Duration {
        let start = Instant::now();
        rt.block_on(async {
            let mut set = tokio::task::JoinSet::new();
            let mut next_index = 0;

            for _ in 0..parallelism.min(buffers.len()) {
                spawn_next_encode_task(
                    &mut set,
                    &mut next_index,
                    buffers,
                    base_path,
                    image::ImageFormat::Png,
                    false,
                    100,
                );
            }

            while let Some(join_result) = set.join_next().await {
                join_result.unwrap().unwrap();
                spawn_next_encode_task(
                    &mut set,
                    &mut next_index,
                    buffers,
                    base_path,
                    image::ImageFormat::Png,
                    false,
                    100,
                );
            }
        });
        start.elapsed()
    }

    // 出力時間の比較ベンチマーク
    // 実行: cargo test --release compare_export_strategies -- --ignored --nocapture
    // (デバッグビルドは画像エンコードが大幅に遅くなるため --release を推奨)
    #[ignore = "ローカルのテスト用GIFファイルが必要なため通常のテストでは実行しない"]
    #[test]
    fn compare_export_strategies() {
        let buffers = load_test_buffers();
        let parallelism = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        println!(
            "frame count: {}, parallelism: {}",
            buffers.len(),
            parallelism
        );

        let dir = std::env::temp_dir().join("gif_ide_export_bench");
        std::fs::create_dir_all(&dir).unwrap();
        let base_path = dir.join("output.png");

        let rt = tokio::runtime::Runtime::new().unwrap();

        let sequential = export_sequential(&buffers, &base_path);
        let full_parallel = export_full_parallel(&rt, &buffers, &base_path);
        let chunked = export_chunked(&rt, &buffers, &base_path, parallelism);
        let streaming = export_streaming(&rt, &buffers, &base_path, parallelism);

        println!("1. 1枚ずつ逐次出力:             {sequential:?}");
        println!("2. 全フレーム並行出力 (無制限):  {full_parallel:?}");
        println!("3. CPUコア数ごとチャンク並行:    {chunked:?}");
        println!("4. ストリーミング並行 (現在):    {streaming:?}");

        std::fs::remove_dir_all(&dir).ok();
    }
}
