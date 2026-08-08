pub mod error;
pub mod gif_anim;
pub mod input;
pub mod mapping;
pub mod pixer;
pub mod quantize;
pub mod render;
pub mod renders;
pub mod repository;
pub mod template;
pub mod vid_anim;

#[cfg(not(target_arch = "wasm32"))]
pub mod anim;
#[cfg(not(target_arch = "wasm32"))]
pub mod webp_anim;

#[cfg(not(target_arch = "wasm32"))]
pub use anim::{AnimEncoder, OutputFormat};
pub use error::{Error, Result};
pub use gif_anim::GifAnim;
pub use input::{Input, Inputs, TextOptions};
pub use mapping::Mapping;
pub use render::Render;
pub use renders::Renders;
pub use repository::Repository;
pub use template::Template;
pub use vid_anim::VidAnim;
#[cfg(not(target_arch = "wasm32"))]
pub use webp_anim::{WebpAnim, WebpOptions};
