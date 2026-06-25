use crate::gif_data::{Gif, GifFile};
use crate::window::{show_message_dialog, show_window};
use crate::{AppWindow, ExportFileWindow, LoadingState};
use rfd::AsyncFileDialog;
use slint::{ComponentHandle, Model, SharedString};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

async fn save_gif_file() -> Option<PathBuf> {
    AsyncFileDialog::new()
        .add_filter("GIF", &["gif"])
        .set_title(crate::i18n::t(
            "保存先を選択してください",
            "Select a destination",
        ))
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
        .set_title(crate::i18n::t(
            "保存先を選択してください",
            "Select a destination",
        ))
        .save_file()
        .await
        .map(|handle| handle.path().to_path_buf())
}

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

pub(crate) fn register_callbacks(
    ui: &AppWindow,
    export_window: &ExportFileWindow,
    gif_file_ref: &Arc<Mutex<Option<GifFile>>>,
) {
    // 出力ウィンドウCallback
    let ui_weak_start_export = ui.as_weak();
    let export_window_weak_start_export = export_window.as_weak();
    let gif_ref_ok = gif_file_ref.clone();
    export_window.on_start_export(move || {
        let (Some(export_window), Some(ui)) = (
            export_window_weak_start_export.upgrade(),
            ui_weak_start_export.upgrade(),
        ) else {
            return;
        };

        let path = PathBuf::from(export_window.get_export_path().as_str());
        let export_window_weak_start_export = export_window.as_weak();
        let ui_weak_start_export = ui.as_weak();

        enum ExportJob {
            Gif {
                gif: GifFile,
                loop_forever: bool,
                delays: Vec<u16>,
                optimize: bool,
            },
            Image {
                buffers: Vec<slint::SharedPixelBuffer<slint::Rgba8Pixel>>,
                format: image::ImageFormat,
                is_jpeg: bool,
                quality: u8,
            },
        }

        let job = if export_window.get_is_gif() {
            let Some(gif) = gif_ref_ok.lock().unwrap().clone() else {
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
                optimize: export_window.get_gif_optimize(),
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

        export_window.set_state(LoadingState::Processing);

        match job {
            ExportJob::Gif {
                gif,
                loop_forever,
                delays,
                optimize,
            } => {
                let total = gif.frames().len();
                export_window.set_progress_current(0);
                export_window.set_progress_total(total as i32);

                let export_window_weak_progress = export_window.as_weak();
                tokio::task::spawn_blocking(move || {
                    let step = (total / 100).max(1);
                    let mut last_reported = 0usize;
                    let mut on_progress = move |n: usize| {
                        if n != total && n - last_reported < step {
                            return;
                        }
                        last_reported = n;
                        let export_window_weak_progress = export_window_weak_progress.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = export_window_weak_progress.upgrade() {
                                w.set_progress_current(n as i32);
                            }
                        });
                    };

                    let result = if optimize {
                        gif.export_optimized(&path, loop_forever, &delays, Some(&mut on_progress))
                    } else {
                        gif.export(&path, loop_forever, &delays, Some(&mut on_progress))
                    };

                    let _ = slint::invoke_from_event_loop(move || match result {
                        Ok(()) => {
                            if let Some(export_window) = export_window_weak_start_export.upgrade() {
                                export_window.set_state(LoadingState::Success);
                            }
                        }
                        Err(e) => {
                            if let Some(ui) = ui_weak_start_export.upgrade() {
                                let message = if crate::i18n::is_english() {
                                    format!("Failed to export GIF: {}", e)
                                } else {
                                    format!("GIFの出力に失敗しました: {}", e)
                                };
                                show_message_dialog(
                                    crate::i18n::t("エラー", "Error"),
                                    &message,
                                    &ui,
                                );
                            }
                        }
                    });
                });
            }
            ExportJob::Image {
                buffers,
                format,
                is_jpeg,
                quality,
            } => {
                let total = buffers.len();
                export_window.set_progress_current(0);
                export_window.set_progress_total(total as i32);

                let export_window_weak_progress = export_window.as_weak();
                tokio::spawn(async move {
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

                    let mut completed = 0usize;
                    let mut result: image::ImageResult<()> = Ok(());
                    while let Some(join_result) = set.join_next().await {
                        match join_result.unwrap() {
                            Ok(()) => {
                                completed += 1;
                                let export_window_weak_progress =
                                    export_window_weak_progress.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(w) = export_window_weak_progress.upgrade() {
                                        w.set_progress_current(completed as i32);
                                    }
                                });
                                spawn_next_encode_task(
                                    &mut set,
                                    &mut next_index,
                                    &buffers,
                                    &path,
                                    format,
                                    is_jpeg,
                                    quality,
                                )
                            }
                            Err(e) => {
                                if result.is_ok() {
                                    result = Err(e);
                                }
                            }
                        }
                    }

                    let _ = slint::invoke_from_event_loop(move || match result {
                        Ok(()) => {
                            if let Some(export_window) = export_window_weak_start_export.upgrade() {
                                export_window.set_state(LoadingState::Success);
                            }
                        }
                        Err(e) => {
                            if let Some(ui) = ui_weak_start_export.upgrade() {
                                let message = if crate::i18n::is_english() {
                                    format!("Failed to export image: {}", e)
                                } else {
                                    format!("画像の出力に失敗しました: {}", e)
                                };
                                show_message_dialog(
                                    crate::i18n::t("エラー", "Error"),
                                    &message,
                                    &ui,
                                );
                            }
                        }
                    });
                });
            }
        }
    });

    let export_window_weak_cancel = export_window.as_weak();
    export_window.on_cancel(move || {
        if let Some(d) = export_window_weak_cancel.upgrade() {
            d.hide().unwrap();
        }
    });

    let export_window_weak_open_export_folder = export_window.as_weak();
    export_window.on_open_export_folder(move || {
        let Some(export_window) = export_window_weak_open_export_folder.upgrade() else {
            return;
        };
        let path = PathBuf::from(export_window.get_export_path().as_str());
        open_in_explorer(&path);
    });

    let export_window_weak_select_export_path = export_window.as_weak();
    export_window.on_select_export_path(move || {
        let Some(export_window) = export_window_weak_select_export_path.upgrade() else {
            return;
        };

        let format_index = export_window.get_format_index();
        let is_gif = export_window.get_is_gif();
        let export_window_weak_select_export_path = export_window.as_weak();
        tokio::spawn(async move {
            let path = if is_gif {
                save_gif_file().await
            } else {
                save_image_file(format_index - 1).await
            };

            let _ = slint::invoke_from_event_loop(move || {
                let Some(export_window) = export_window_weak_select_export_path.upgrade() else {
                    return;
                };
                if let Some(path) = path {
                    export_window
                        .set_export_path(SharedString::from(path.to_string_lossy().into_owned()));
                }
            });
        });
    });

    // 画像出力Callback
    let ui_weak_export_file = ui.as_weak();
    let export_window_weak_export_file = export_window.as_weak();
    ui.on_export_file(move || {
        let (Some(ui), Some(export_window)) = (
            ui_weak_export_file.upgrade(),
            export_window_weak_export_file.upgrade(),
        ) else {
            return;
        };

        export_window.set_state(LoadingState::Form);
        show_window!(export_window, ui);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gif_data::Gif;
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
