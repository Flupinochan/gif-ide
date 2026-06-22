use crate::ffmpeg::{Ffmpeg, VideoMetadata};
use crate::FrameData;
use anyhow::Result;
use rayon::prelude::*;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::fs::File;
use std::io::{BufWriter, ErrorKind, Read};
use std::path::Path;

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
        let ffmpeg = Ffmpeg::new()?;

        // メタデータを取得
        let VideoMetadata {
            width,
            height,
            delay,
        } = ffmpeg.get_video_metadata(path)?;

        // フレーム本体を取得
        let mut child = ffmpeg.spawn_raw_frames(path)?;

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

    pub fn retain_frames(&mut self, interval: i32, start_index: i32) {
        let mut idx: i32 = 0;
        self.frames.retain(|_| {
            let keep = idx % interval != start_index - 1;
            idx += 1;
            keep
        });
    }

    pub fn resize_canvas(
        &mut self,
        new_width: u16,
        new_height: u16,
        filter_type: image::imageops::FilterType,
    ) {
        let scale_x = new_width as f64 / self.canvas_width as f64;
        let scale_y = new_height as f64 / self.canvas_height as f64;

        self.frames.par_iter_mut().for_each(|frame| {
            let new_frame_width = ((frame.width as f64 * scale_x).round() as u16)
                .max(1)
                .min(new_width);
            let new_frame_height = ((frame.height as f64 * scale_y).round() as u16)
                .max(1)
                .min(new_height);
            let new_left =
                ((frame.left as f64 * scale_x).round() as u16).min(new_width - new_frame_width);
            let new_top =
                ((frame.top as f64 * scale_y).round() as u16).min(new_height - new_frame_height);

            frame.pixels = resize_frame_pixels(
                &frame.pixels,
                frame.width,
                frame.height,
                new_frame_width,
                new_frame_height,
                filter_type,
            );
            frame.width = new_frame_width;
            frame.height = new_frame_height;
            frame.left = new_left;
            frame.top = new_top;
        });

        self.canvas_width = new_width;
        self.canvas_height = new_height;
    }

    pub fn frames_mut(&mut self) -> &mut [GifFrame] {
        &mut self.frames
    }

    // 生フレームからUI用モデルを構築
    pub fn build_frame_data(&self) -> Vec<FrameData> {
        self.frames()
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                self.frame_image(i).map(|img| FrameData {
                    image: img,
                    delay: (f.delay as i32).max(2),
                })
            })
            .collect()
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

fn resize_frame_pixels(
    pixels: &[u8],
    width: u16,
    height: u16,
    new_width: u16,
    new_height: u16,
    filter_type: image::imageops::FilterType,
) -> Vec<u8> {
    let img = image::RgbaImage::from_raw(width as u32, height as u32, pixels.to_vec())
        .expect("invalid image buffer");
    image::imageops::resize(&img, new_width as u32, new_height as u32, filter_type).into_raw()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    // 並列化前の実装 (比較用に保持)
    fn resize_canvas_sequential(
        gif: &mut GifFile,
        new_width: u16,
        new_height: u16,
        filter_type: image::imageops::FilterType,
    ) {
        let scale_x = new_width as f64 / gif.canvas_width as f64;
        let scale_y = new_height as f64 / gif.canvas_height as f64;

        for frame in &mut gif.frames {
            let new_frame_width = ((frame.width as f64 * scale_x).round() as u16)
                .max(1)
                .min(new_width);
            let new_frame_height = ((frame.height as f64 * scale_y).round() as u16)
                .max(1)
                .min(new_height);
            let new_left =
                ((frame.left as f64 * scale_x).round() as u16).min(new_width - new_frame_width);
            let new_top =
                ((frame.top as f64 * scale_y).round() as u16).min(new_height - new_frame_height);

            frame.pixels = resize_frame_pixels(
                &frame.pixels,
                frame.width,
                frame.height,
                new_frame_width,
                new_frame_height,
                filter_type,
            );
            frame.width = new_frame_width;
            frame.height = new_frame_height;
            frame.left = new_left;
            frame.top = new_top;
        }

        gif.canvas_width = new_width;
        gif.canvas_height = new_height;
    }

    // ui/test.gifを幅1000pxにリサイズする際の、並列化前/後の処理時間比較
    // 実行: cargo test --release compare_resize_canvas_strategies -- --ignored --nocapture
    #[ignore = "ローカルのui/test.gifを使うベンチマークのため通常のテストでは実行しない"]
    #[test]
    fn compare_resize_canvas_strategies() {
        let gif = GifFile::new(Path::new("ui/test.gif")).expect("test.gifの読み込みに失敗");

        let new_width: u16 = 1000;
        let new_height = ((new_width as f64) * gif.canvas_height as f64 / gif.canvas_width as f64)
            .round() as u16;

        println!(
            "frame count: {}, {}x{} -> {}x{}",
            gif.frames.len(),
            gif.canvas_width,
            gif.canvas_height,
            new_width,
            new_height
        );

        let mut sequential_gif = gif.clone();
        let start = Instant::now();
        resize_canvas_sequential(
            &mut sequential_gif,
            new_width,
            new_height,
            image::imageops::FilterType::Lanczos3,
        );
        println!("逐次 (並列化前): {:?}", start.elapsed());

        let mut parallel_gif = gif.clone();
        let start = Instant::now();
        parallel_gif.resize_canvas(new_width, new_height, image::imageops::FilterType::Lanczos3);
        println!("並列 (rayon, 並列化後): {:?}", start.elapsed());
    }
}
