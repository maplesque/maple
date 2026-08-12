pub use maple_render_core::{
    Error, GifAnim, Input, Inputs, Mapping, Render, Renders, Repository, Result, Template,
    TextOptions, error, gif_anim, input, mapping, pixer, quantize, render, renders, repository,
    template, vid_anim,
};
#[cfg(not(target_arch = "wasm32"))]
pub use maple_render_core::{
    WebpAnim, WebpAnimationOptions, WebpEncoder, WebpFrame, WebpOptions, encode_webp_animation,
    webp_anim,
};

#[cfg(target_arch = "wasm32")]
pub mod wasm;
