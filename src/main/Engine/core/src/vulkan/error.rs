use std::fmt;

/// Error de inicialización o de frame Vulkan (mensajes en español, para el log y el editor).
#[derive(Debug, Clone)]
pub struct VulkanError {
    pub message: String,
}

impl VulkanError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for VulkanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for VulkanError {}

pub fn vk_error(action: &str, result: ash::vk::Result) -> VulkanError {
    VulkanError::new(format!(
        "No se pudo {action} (Vulkan {result:?}).\n\n\
         Sugerencias:\n\
         • Instala o actualiza los controladores de tu tarjeta gráfica (NVIDIA / AMD / Intel).\n\
         • Comprueba Vulkan en terminal: `vulkaninfo` o `vkcube`.\n\
         • Reinicia el editor después de instalar drivers."
    ))
}
