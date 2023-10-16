#[derive(Clone)]
pub struct MatrixConfiguration {
    pub width: usize,
    pub height: usize,
    pub target_fps: f32,
}

impl Default for MatrixConfiguration {
    fn default() -> Self {
        Self {
            width: 12,
            height: 12,
            target_fps: 30.0,
        }
    }
}
