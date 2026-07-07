// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_window;
mod edit_canvas_resize_window;
mod edit_frame_drop_window;
mod export_window;
mod ffmpeg;
mod gif_data;
mod i18n;
mod import_window;
mod logging;
mod window;

use anyhow::Result;
use std::sync::{Arc, Mutex};

use crate::gif_data::GifFile;

slint::include_modules!();

/// available_parallelism の半分 (最低 1) を返す。
/// rayon グローバルプールと tokio JoinSet の並列数を揃えるための共通ヘルパー。
pub(crate) fn half_parallelism() -> usize {
    (std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        / 2)
    .max(1)
}

fn main() -> std::process::ExitCode {
    logging::init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting gif-ide");

    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = ?e, "gif-ide exited with an error");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(half_parallelism())
        .build_global()
        .ok();

    let rt = tokio::runtime::Runtime::new()?;
    let _guard = rt.enter();

    let ui = AppWindow::new()?;
    logging::warn_on_err(
        slint::select_bundled_translation(ui.get_current_language().as_str()),
        "select_bundled_translation failed",
    );
    crate::i18n::IS_ENGLISH.store(
        ui.get_current_language() == "en",
        std::sync::atomic::Ordering::Relaxed,
    );
    let export_window = ExportFileWindow::new()?;
    let import_window = ImportFileWindow::new()?;
    let edit_frame_drop_window = EditFrameDropWindow::new()?;
    let edit_canvas_resize_window = EditCanvasResizeWindow::new()?;

    let gif_file_ref: Arc<Mutex<Option<GifFile>>> = Arc::new(Mutex::new(None));

    app_window::register_callbacks(&ui, &gif_file_ref);
    import_window::register_callbacks(&ui, &import_window, &gif_file_ref);
    export_window::register_callbacks(&ui, &export_window, &gif_file_ref);
    edit_frame_drop_window::register_callbacks(&ui, &edit_frame_drop_window, &gif_file_ref);
    edit_canvas_resize_window::register_callbacks(&ui, &edit_canvas_resize_window, &gif_file_ref);

    if let Err(err) = ui.run() {
        // VM/RDP環境等、GPUドライバが無くOpenGLの初期化に失敗する場合がある。
        // 実際の初期化はui.run()のイベントループ開始後に行われ、かつSlintは
        // 一度確定したプラットフォームを同一プロセス内で切り替えられないため、
        // ソフトウェアレンダラーを強制する環境変数を付けて自プロセスを再起動する
        const RETRY_GUARD: &str = "GIF_IDE_SOFTWARE_RENDERER_RETRY";
        if std::env::var_os(RETRY_GUARD).is_none() {
            tracing::error!(error = %err, "ui.run() failed, retrying with software renderer");
            let exe = std::env::current_exe()?;
            let status = std::process::Command::new(exe)
                .env("SLINT_BACKEND", "winit-software")
                .env(RETRY_GUARD, "1")
                .status()?;
            std::process::exit(status.code().unwrap_or(1));
        }
        return Err(err.into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::{Model, Timer, TimerMode};
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    // 動画読込→フレーム間引き→PNG全フレーム出力を実際のcallback (invoke_*) で連続実行し、
    // その間UIスレッド (イベントループ) が一定間隔で動き続けているか (固まっていないか) を検証する。
    // 環境変数 TEST_VIDEO_PATH (動画) または TEST_GIF_PATH (GIF) にファイルパスを指定して実行すること。
    // デバッグビルドは画像処理が大幅に遅くなり閾値判定が不安定になるため --release を推奨。
    // 注意: cargo test はワーカースレッドでテストを実行するが、macOS/Windows では Slint の
    // イベントループは OS メインスレッドでのみ安定動作する。クラッシュする場合は専用バイナリで実行すること。
    // 実行: cargo test --release verify_ui_thread_not_blocked -- --ignored --nocapture
    #[ignore = "実際のウィンドウを起動して検証するため通常のテストでは実行しない"]
    #[test]
    fn verify_ui_thread_not_blocked() {
        // 動画: TEST_VIDEO_PATH (format-index=1) / GIF: TEST_GIF_PATH (format-index=0) のどちらかを指定する
        let (import_path, import_format_index) = match std::env::var("TEST_VIDEO_PATH") {
            Ok(p) => (p, 1),
            Err(_) => (
                std::env::var("TEST_GIF_PATH").expect(
                    "環境変数 TEST_VIDEO_PATH (動画) または TEST_GIF_PATH (GIF) を指定してください",
                ),
                0,
            ),
        };
        let export_dir = std::env::temp_dir().join("gif_ide_ui_check");
        std::fs::create_dir_all(&export_dir).unwrap();
        let export_path = export_dir.join("frame.png");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();

        let ui = AppWindow::new().unwrap();
        let export_window = ExportFileWindow::new().unwrap();
        let import_window = ImportFileWindow::new().unwrap();
        let edit_frame_drop_window = EditFrameDropWindow::new().unwrap();
        let edit_canvas_resize_window = EditCanvasResizeWindow::new().unwrap();
        let gif_file_ref: Arc<Mutex<Option<GifFile>>> = Arc::new(Mutex::new(None));

        app_window::register_callbacks(&ui, &gif_file_ref);
        import_window::register_callbacks(&ui, &import_window, &gif_file_ref);
        export_window::register_callbacks(&ui, &export_window, &gif_file_ref);
        edit_frame_drop_window::register_callbacks(&ui, &edit_frame_drop_window, &gif_file_ref);
        edit_canvas_resize_window::register_callbacks(
            &ui,
            &edit_canvas_resize_window,
            &gif_file_ref,
        );

        // フェーズを進めるポーリングタイマー (動画読込 → フレーム間引き → PNG全フレーム出力)
        let phase: Rc<Cell<u8>> = Rc::new(Cell::new(0));

        // UIスレッドのハートビート計測: 5ms間隔でtickし、間隔が大きく空けば
        // その間UIスレッド (イベントループ) がブロックされていたとみなす (どのphase中かも記録する)
        let heartbeats: Rc<RefCell<Vec<(Instant, u8)>>> = Rc::new(RefCell::new(Vec::new()));
        let heartbeats_tick = heartbeats.clone();
        let phase_for_heartbeat = phase.clone();
        let heartbeat_timer = Timer::default();
        heartbeat_timer.start(TimerMode::Repeated, Duration::from_millis(5), move || {
            heartbeats_tick
                .borrow_mut()
                .push((Instant::now(), phase_for_heartbeat.get()));
        });

        let phase_poll = phase.clone();
        let deadline = Instant::now() + Duration::from_secs(60);
        let ui_weak = ui.as_weak();
        let import_window_weak = import_window.as_weak();
        let edit_frame_drop_window_weak = edit_frame_drop_window.as_weak();
        let export_window_weak = export_window.as_weak();
        let import_path_owned = import_path.clone();
        let export_path_owned: PathBuf = export_path.clone();
        let driver_timer = Timer::default();
        driver_timer.start(TimerMode::Repeated, Duration::from_millis(20), move || {
            let Some(ui) = ui_weak.upgrade() else { return };

            if Instant::now() > deadline {
                eprintln!(
                    "タイムアウト: phase={} is_loading={} frames={}",
                    phase_poll.get(),
                    ui.get_is_loading(),
                    ui.get_frames().row_count()
                );
                let _ = slint::quit_event_loop();
                return;
            }

            match phase_poll.get() {
                // 動画/GIF読込開始
                0 => {
                    let Some(import_window) = import_window_weak.upgrade() else {
                        return;
                    };
                    ui.invoke_import_file();
                    import_window.set_format_index(import_format_index);
                    import_window.set_import_path(import_path_owned.clone().into());
                    import_window.invoke_start_import();
                    phase_poll.set(1);
                }
                // フレーム間引き開始
                1 => {
                    if !ui.get_is_loading() && ui.get_frames().row_count() > 0 {
                        let Some(drop_window) = edit_frame_drop_window_weak.upgrade() else {
                            return;
                        };
                        ui.invoke_edit_frame_drop();
                        drop_window.set_frame_drop_interval(2);
                        drop_window.set_frame_drop_start_index(1);
                        drop_window.invoke_start_frame_drop();
                        phase_poll.set(2);
                    }
                }
                // PNG全フレーム出力開始
                2 => {
                    let Some(drop_window) = edit_frame_drop_window_weak.upgrade() else {
                        return;
                    };
                    if drop_window.get_state() == LoadingState::Success {
                        let Some(export_window) = export_window_weak.upgrade() else {
                            return;
                        };
                        ui.invoke_export_file();
                        export_window.set_format_index(1);
                        export_window.set_range_index(1);
                        export_window.set_export_path(
                            export_path_owned.to_string_lossy().into_owned().into(),
                        );
                        export_window.invoke_start_export();
                        phase_poll.set(3);
                    }
                }
                // 完了
                3 => {
                    let Some(export_window) = export_window_weak.upgrade() else {
                        return;
                    };
                    if export_window.get_state() == LoadingState::Success {
                        phase_poll.set(4);
                        let _ = slint::quit_event_loop();
                    }
                }
                _ => {}
            }
        });

        ui.run().unwrap();

        let _ = std::fs::remove_dir_all(&export_dir);

        assert_eq!(
            phase.get(),
            4,
            "全フェーズ (動画読込→フレーム間引き→PNG出力) が完了しませんでした"
        );

        let beats = heartbeats.borrow();
        // phase 0 (動画/GIFの読込開始前、ウィンドウ生成直後の初期化) は計測対象外とする
        let (max_gap, max_gap_phase) = beats
            .windows(2)
            .filter(|w| w[0].1 != 0)
            .map(|w| (w[1].0.duration_since(w[0].0), w[0].1))
            .max_by_key(|(gap, _)| *gap)
            .unwrap_or((Duration::ZERO, 0));
        println!(
            "heartbeat count: {}, max gap: {:?} (phase {} 中に発生)",
            beats.len(),
            max_gap,
            max_gap_phase
        );
        // 実測ベースの閾値: スレッド生成等のOS要因で150〜250ms程度のノイズが乗ることがあるため、
        // 「コード上の問題で数秒間固まる」という本来の不具合と区別できるラインとして400msを採用
        assert!(
            max_gap < Duration::from_millis(400),
            "UIスレッドが{max_gap:?}ブロックされました (heartbeatタイマーが発火しませんでした)"
        );
    }
}
