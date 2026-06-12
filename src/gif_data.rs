use anyhow::Result;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::fs::File;
use std::io::BufWriter;
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

    // UIでカスタム設定として実装予定の箇所をTODOとして記載
    pub fn export(&self, path: &Path, loop_forever: bool) -> Result<()> {
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

        for frame in &self.frames {
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
            gif_frame.delay = frame.delay;
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
