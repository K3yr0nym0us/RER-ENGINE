pub struct GridConfig {
    /// Conservado por IPC/guardado (2D); en 3D la cuadrícula visible es el checker del suelo.
    pub visible: bool,
    /// Tamaño de celda para snap (Ctrl) y UV del checker del suelo.
    pub cell_size: f32,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            visible: true,
            cell_size: 1.0,
        }
    }
}
