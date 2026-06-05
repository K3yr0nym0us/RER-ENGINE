#[derive(Debug, Clone)]
pub struct EntitySnapshot {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub facing_right: bool,
    pub facing_sign: f32,
    pub animations: Vec<String>,
}
