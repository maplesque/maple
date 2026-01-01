use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub delay: f64,
    pub hold: f64,
    pub palette: Vec<i32>,
}

impl Default for Template {
    fn default() -> Self {
        Template { width: 400, height: 300, frames: 1, delay: 0.1, hold: 1.0, palette: vec![0] }
    }
}

impl Template {
    pub fn period(&self) -> f64 {
        self.delay
    }

    pub fn is_animation(&self) -> bool {
        self.frames > 1
    }
}
