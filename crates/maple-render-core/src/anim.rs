//! Common trait for animation encoders (GIF / WebP / Video).
//!
//! Each encoder consumes a [`Renders`] stream of fully-rasterized frames and
//! produces its own binary format. The trait lets the CLI dispatch on an
//! `OutputFormat` enum without duplicating orchestration logic.

use std::path::Path;

use crate::error::Result;

/// The binary container format to emit for an animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OutputFormat {
    Gif,
    Webp,
    Video,
}

/// Something that can consume rendered frames and emit an encoded animation.
pub trait AnimEncoder {
    /// Encode all frames into the target format, returning the bytes.
    fn encode(&mut self) -> Result<Vec<u8>>;

    /// Encode all frames and write them to `path`.
    fn save<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let data = self.encode()?;
        let mut file = std::fs::File::create(path.as_ref())?;
        std::io::Write::write_all(&mut file, &data)?;
        Ok(())
    }
}
