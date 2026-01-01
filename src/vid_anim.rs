use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

use crate::{
    error::{Error, Result},
    renders::Renders,
};

pub struct VidAnim {
    renders: Renders,
    period: f64,
    hold: f64,
    first_frame: i32,
}

impl VidAnim {
    pub fn new(renders: Renders) -> Self {
        VidAnim { renders, period: 0.1, hold: 5.0, first_frame: -1 }
    }

    pub fn set_first_frame(&mut self, index: i32) {
        self.first_frame = index;
    }

    pub fn set_timing(&mut self, period: f64, hold: f64) {
        self.period = period;
        self.hold = hold;
    }

    pub fn apply<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let rate = 30;
        let frame_rate = 1.0 / rate as f64;

        let frames = self.renders.length() as i32;
        if frames == 0 {
            return Err(Error::VideoEncode("No frames to encode".to_string()));
        }

        let first_render = self.renders.get_render(0)?;
        let width = first_render.get().width();
        let height = first_render.get().height();

        let mut ffmpeg = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgb24",
                "-video_size",
                &format!("{}x{}", width, height),
                "-framerate",
                &rate.to_string(),
                "-i",
                "-",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-preset",
                "medium",
                "-crf",
                "23",
            ])
            .arg(path.as_ref())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| Error::VideoEncode(format!("Failed to start ffmpeg: {}", e)))?;

        let mut stdin = ffmpeg
            .stdin
            .take()
            .ok_or_else(|| Error::VideoEncode("Failed to open ffmpeg stdin".to_string()))?;

        let mut curr = 0.0f64;
        let mut target = -1.0f64;
        let mut base = 0;
        let mut current_frame_data: Option<Vec<u8>> = None;

        loop {
            let i = if self.first_frame >= 0 { (base + self.first_frame) % frames } else { base };

            if curr >= target {
                if base >= frames {
                    break;
                }

                let step = if i == frames - 1 { self.period + self.hold } else { self.period };
                target = curr + step;

                let render = self.renders.get_render(i)?;
                let img = render.get();

                let mut rgb_data = Vec::with_capacity((width * height * 3) as usize);
                for pixel in img.pixels() {
                    rgb_data.push(pixel[0]);
                    rgb_data.push(pixel[1]);
                    rgb_data.push(pixel[2]);
                }
                current_frame_data = Some(rgb_data);

                self.renders.remove_render(i);
                base += 1;
            }

            // Write current frame
            if let Some(ref data) = current_frame_data {
                stdin
                    .write_all(data)
                    .map_err(|e| Error::VideoEncode(format!("Failed to write frame: {}", e)))?;
            }

            curr += frame_rate;
        }

        drop(stdin);

        let status = ffmpeg
            .wait()
            .map_err(|e| Error::VideoEncode(format!("Failed to wait for ffmpeg: {}", e)))?;

        if !status.success() {
            return Err(Error::VideoEncode("ffmpeg exited with error".to_string()));
        }

        Ok(())
    }
}
