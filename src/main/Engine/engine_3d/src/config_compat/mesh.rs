pub struct GridConfig {
    pub world_width: f32,
    pub world_height: f32,
    pub visible: bool,
    pub cell_size: f32,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            world_width: 100.0,
            world_height: 50.0,
            visible: false,
            cell_size: 1.0,
        }
    }
}
