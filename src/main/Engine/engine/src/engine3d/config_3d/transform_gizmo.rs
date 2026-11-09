//! Modo del gizmo de transformación en el viewport (traslación / rotación).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransformGizmoMode {
    #[default]
    Translate,
    Rotate,
}

impl TransformGizmoMode {
    pub fn toggle(self) -> Self {
        match self {
            Self::Translate => Self::Rotate,
            Self::Rotate => Self::Translate,
        }
    }
}
