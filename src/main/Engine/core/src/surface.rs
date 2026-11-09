//! Resultado de presentación de la superficie. El enum lo comparten los dos motores (2D y 3D)
//! para decidir si reconfigurar, saltar un frame o continuar; el backend real es Ash/Vulkan.

/// Resultado al pedir el frame actual de la superficie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePresentError {
    /// Reconfigurar la superficie (p. ej. `resize` + `configure`).
    Reconfigure,
    /// Saltar este frame (timeout, ocluida, etc.).
    SkipFrame,
    /// Error de validación.
    Validation,
}
