use std::sync::Mutex;

// %LOCALAPPDATA%配下 (書き込み権限が常に保証される、Program Filesインストール時も安全) に
// 固定ファイル名でログを出力する。起動の度に上書きし、1ファイルのみ保持する
fn log_path() -> std::path::PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("gif-ide")
        .join("gif-ide.log")
}

// ロギング初期化に失敗してもアプリ本体は継続動作させる (診断機能のため非致命的)
pub(crate) fn init() {
    if let Err(e) = try_init() {
        eprintln!("failed to initialize file logging: {e}");
    }
}

fn try_init() -> anyhow::Result<()> {
    let path = log_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)?;

    tracing_subscriber::fmt()
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    // panic (unwrap/expect等) もログファイルに記録する。デフォルトのhookも維持し、
    // デバッグ実行時 (コンソールあり) は従来通り標準エラーにも出力させる
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        tracing::error!("{panic_info}");
        default_hook(panic_info);
    }));

    Ok(())
}

// slint::invoke_from_event_loop等、Resultを握りつぶしていた箇所をまとめて記録するヘルパー
pub(crate) fn warn_on_err<E: std::fmt::Display>(result: Result<(), E>, context: &str) {
    if let Err(e) = result {
        tracing::warn!(error = %e, "{context}");
    }
}
