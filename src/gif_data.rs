use anyhow::Result;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::fs::File;
use std::path::Path;

pub struct GifFrame {
    pub pixels: Vec<u8>,
    pub width: u16,
    pub height: u16,
    pub left: u16,
    pub top: u16,
    pub delay: u16,
    pub dispose: gif::DisposalMethod,
}

pub struct GifFile {
    frames: Vec<GifFrame>,
    pub canvas_width: u16,
    pub canvas_height: u16,
}

pub trait Gif {
    fn frame_count(&self) -> usize;
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
}

impl Gif for GifFile {
    fn frame_count(&self) -> usize {
        self.frames.len()
    }

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
