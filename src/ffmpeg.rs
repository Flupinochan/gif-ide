use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

pub(crate) struct Ffmpeg {
    ffmpeg_path: PathBuf,
    ffprobe_path: PathBuf,
}

impl Ffmpeg {
    pub(crate) fn new() -> Result<Self> {
        Ok(Self {
            ffmpeg_path: get_ffmpeg_path("ffmpeg.exe")?,
            ffprobe_path: get_ffmpeg_path("ffprobe.exe")?,
        })
    }

    // ffprobeによる動画メタデータ (解像度・フレームレート) の取得
    pub(crate) fn get_video_metadata(&self, path: &Path) -> Result<VideoMetadata> {
        let output = Command::new(&self.ffprobe_path)
            .args([
                "-loglevel",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=width,height,r_frame_rate",
                "-of",
                "default=noprint_wrappers=1",
            ])
            .arg(path)
            .output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "ffprobeの実行に失敗しました: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        parse_ffprobe_output(&String::from_utf8_lossy(&output.stdout))
    }

    // ffmpegで動画を生のRGBAフレーム列に変換するプロセスを起動
    pub(crate) fn spawn_raw_frames(&self, path: &Path) -> Result<Child> {
        Ok(Command::new(&self.ffmpeg_path)
            .args(["-loglevel", "error", "-i"])
            .arg(path)
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?)
    }

    // concatデマルチプレクサのmanifest (PNG連番+各フレームのduration) を読み込み、
    // paletteuse(diff_mode=rectangle)で差分矩形のみ出力するGIFとしてエンコードするプロセスを起動
    pub(crate) fn spawn_gif_encoder(
        &self,
        manifest_path: &Path,
        loop_forever: bool,
        final_delay: u16,
        output_path: &Path,
    ) -> Result<Child> {
        let loop_value = if loop_forever { "0" } else { "-1" };

        Ok(Command::new(&self.ffmpeg_path)
            .args(["-loglevel", "error"])
            .args(["-f", "concat"])
            .args(["-safe", "0"])
            .arg("-i")
            .arg(manifest_path)
            .args(["-fps_mode", "passthrough"])
            .args([
                "-lavfi",
                "split[a][b];\
                 [a]palettegen=max_colors=256:reserve_transparent=1:stats_mode=diff[p];\
                 [b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle",
            ])
            .args(["-loop", loop_value])
            .arg("-final_delay")
            .arg(final_delay.to_string())
            .args(["-y"])
            .arg(output_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?)
    }
}

// 優先: 実行ファイル同階層の `ffmpeg/<name>` / フォールバック (開発時): `CARGO_MANIFEST_DIR/resources/ffmpeg/<name>`
//
// TODO: 配布用ビルドではresources/ffmpeg/*.exeを実行ファイルと同階層の`ffmpeg/`に
//       配置する手順を別途整備する (build.rsでの自動コピーは未対応)
fn get_ffmpeg_path(name: &str) -> Result<PathBuf> {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            let candidate = dir.join("ffmpeg").join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    #[cfg(debug_assertions)]
    {
        let candidate = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("ffmpeg")
            .join(name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(anyhow::anyhow!(
        "{name} が見つかりません。resources/ffmpeg/ に配置してください"
    ))
}

// ffprobeから取得した動画のメタデータ
pub(crate) struct VideoMetadata {
    pub(crate) width: u16,
    pub(crate) height: u16,
    pub(crate) delay: u16,
}

// ffprobeの `-of default=noprint_wrappers=1` 出力 (key=value形式) のパース
fn parse_ffprobe_output(output: &str) -> Result<VideoMetadata> {
    let mut width = None;
    let mut height = None;
    let mut delay = None;

    for line in output.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "width" => width = value.trim().parse::<u16>().ok(),
            "height" => height = value.trim().parse::<u16>().ok(),
            "r_frame_rate" => {
                delay = value
                    .trim()
                    .split_once('/')
                    .and_then(|(numerator, denominator)| {
                        let numerator: f64 = numerator.parse().ok()?;
                        let denominator: f64 = denominator.parse().ok()?;
                        if denominator == 0.0 {
                            return None;
                        }
                        let fps = numerator / denominator;
                        if fps <= 0.0 {
                            Some(10)
                        } else {
                            Some(
                                (100.0 / fps)
                                    .round()
                                    .clamp(u16::MIN as f64, u16::MAX as f64)
                                    as u16,
                            )
                        }
                    });
            }
            _ => {}
        }
    }

    let width = width.ok_or_else(|| anyhow::anyhow!("widthの取得に失敗しました"))?;
    let height = height.ok_or_else(|| anyhow::anyhow!("heightの取得に失敗しました"))?;
    let delay = delay.ok_or_else(|| anyhow::anyhow!("フレームレートの取得に失敗しました"))?;

    Ok(VideoMetadata {
        width,
        height,
        delay,
    })
}
