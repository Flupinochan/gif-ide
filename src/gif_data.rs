use anyhow::Result;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::fs::File;
use std::io::{BufWriter, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone)]
pub struct GifFrame {
    pub pixels: Vec<u8>,
    pub width: u16,
    pub height: u16,
    pub left: u16,
    pub top: u16,
    pub delay: u16,
    pub dispose: gif::DisposalMethod,
}

#[derive(Clone)]
pub struct GifFile {
    frames: Vec<GifFrame>,
    pub canvas_width: u16,
    pub canvas_height: u16,
}

pub trait Gif {
    fn frames(&self) -> &[GifFrame];
    fn frame_image(&self, index: usize) -> Option<Image>;
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
struct VideoMetadata {
    width: u16,
    height: u16,
    delay: u16,
}

// ffprobeによる動画メタデータ (解像度・フレームレート) の取得
fn get_video_metadata(ffprobe: PathBuf, path: &Path) -> Result<VideoMetadata> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
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

impl GifFile {
    pub fn new(path: &Path) -> Result<Self> {
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let file = File::open(path)?;
        let mut decoder = options.read_info(file)?;
        let mut gif_file = GifFile {
            frames: vec![],
            canvas_height: decoder.height(),
            canvas_width: decoder.width(),
        };
        while let Some(frame) = decoder.read_next_frame()? {
            let gif_frame = GifFrame {
                pixels: frame.buffer.to_vec(),
                width: frame.width,
                height: frame.height,
                left: frame.left,
                top: frame.top,
                delay: frame.delay,
                dispose: frame.dispose,
            };
            gif_file.frames.push(gif_frame);
        }
        Ok(gif_file)
    }

    // 動画ファイルの読み込み (ffmpeg.exe/ffprobe.exeを利用)
    pub fn from_video(path: &Path) -> Result<Self> {
        let ffprobe = get_ffmpeg_path("ffprobe.exe")?;
        let ffmpeg = get_ffmpeg_path("ffmpeg.exe")?;

        // メタデータを取得
        let VideoMetadata {
            width,
            height,
            delay,
        } = get_video_metadata(ffprobe, path)?;

        // フレーム本体を取得
        let mut child = Command::new(ffmpeg)
            .args(["-v", "error", "-i"])
            .arg(path)
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let mut stdout = child.stdout.take().expect("stdoutはpipeで確保済み");
        let frame_size = width as usize * height as usize * 4;
        let mut frames = Vec::new();
        let mut buf = vec![0u8; frame_size];

        loop {
            match stdout.read_exact(&mut buf) {
                Ok(()) => frames.push(GifFrame {
                    pixels: buf.clone(),
                    width,
                    height,
                    left: 0,
                    top: 0,
                    delay,
                    dispose: gif::DisposalMethod::Background,
                }),
                Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
        }

        child.wait()?;

        if frames.is_empty() {
            return Err(anyhow::anyhow!("動画からフレームを取得できませんでした"));
        }

        Ok(GifFile {
            frames,
            canvas_width: width,
            canvas_height: height,
        })
    }

    pub fn export(&self, path: &Path, loop_forever: bool, delays: &[u16]) -> Result<()> {
        let w = self.canvas_width;
        let h = self.canvas_height;
        let file = BufWriter::new(File::create(path)?);
        let mut encoder = gif::Encoder::new(file, w, h, &[])?;
        let repeat = if loop_forever {
            gif::Repeat::Infinite
        } else {
            gif::Repeat::Finite(0)
        };
        encoder.set_repeat(repeat)?;

        // TODO: Canvasサイズ
        let mut canvas = vec![0u8; w as usize * h as usize * 4];

        for (frame, &delay) in self.frames.iter().zip(delays) {
            let prev_canvas = if frame.dispose == gif::DisposalMethod::Previous {
                Some(canvas.clone())
            } else {
                None
            };

            // Frame作成
            for row in 0..frame.height as usize {
                for col in 0..frame.width as usize {
                    let src = (row * frame.width as usize + col) * 4;
                    let dst =
                        ((frame.top as usize + row) * w as usize + frame.left as usize + col) * 4;
                    if frame.pixels[src + 3] > 0 {
                        canvas[dst..dst + 4].copy_from_slice(&frame.pixels[src..src + 4]);
                    }
                }
            }

            let mut pixels = canvas.clone();
            let mut gif_frame = gif::Frame::from_rgba_speed(w, h, &mut pixels, 10);
            gif_frame.delay = delay;
            gif_frame.dispose = gif::DisposalMethod::Background;
            encoder.write_frame(&gif_frame)?;

            // 後処理
            match frame.dispose {
                // canvasを0でfillして透明化
                gif::DisposalMethod::Background => {
                    for row in 0..frame.height as usize {
                        for col in 0..frame.width as usize {
                            let dst = ((frame.top as usize + row) * w as usize
                                + frame.left as usize
                                + col)
                                * 4;
                            canvas[dst..dst + 4].fill(0);
                        }
                    }
                }
                // 前フレームの状態に戻す
                gif::DisposalMethod::Previous => {
                    canvas = prev_canvas.unwrap();
                }
                // TODO: Keepの場合は前フレームと後フレームを差分比較して最小サイズに
                _ => {}
            }
        }
        Ok(())
    }

    /// start_index 番目のフレームを起点として、interval フレームごとに削除 (間引き)
    pub fn retain_frames(&mut self, interval: i32, start_index: i32) {
        let mut idx: i32 = 0;
        self.frames.retain(|_| {
            let keep = idx % interval != start_index - 1;
            idx += 1;
            keep
        });
    }

    pub fn frames_mut(&mut self) -> &mut [GifFrame] {
        &mut self.frames
    }
}

impl Gif for GifFile {
    fn frames(&self) -> &[GifFrame] {
        &self.frames
    }

    fn frame_image(&self, index: usize) -> Option<Image> {
        let frame = self.frames.get(index)?;
        let w = self.canvas_width as u32;
        let h = self.canvas_height as u32;
        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(w, h);
        let pixels = buffer.make_mut_bytes();
        for row in 0..frame.height as u32 {
            for col in 0..frame.width as u32 {
                let src = ((row * frame.width as u32 + col) * 4) as usize;
                let dst = (((frame.top as u32 + row) * w + frame.left as u32 + col) * 4) as usize;
                pixels[dst..dst + 4].copy_from_slice(&frame.pixels[src..src + 4]);
            }
        }
        Some(Image::from_rgba8(buffer))
    }
}
