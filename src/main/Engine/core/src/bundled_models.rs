//! Modelos 3D empaquetados con el motor (no dependen del proyecto del usuario).

/// `model_id` estable del mesh base del jugador por defecto.
pub const DEFAULT_PLAY_CHARACTER_MODEL_ID: &str = "model_male_base_mesh";

/// Archivo fuente FBX junto al binario del motor (`Engine/Models/`).
pub const DEFAULT_PLAY_CHARACTER_FBX: &str = "male_base_mesh.fbx";

/// Bake precalculado del FBX base (misma carpeta que el `.fbx`).
pub const DEFAULT_PLAY_CHARACTER_RERASSET: &str = "male_base_mesh.rerasset";

/// Etiqueta en biblioteca de modelos / Recursos.
pub const DEFAULT_PLAY_CHARACTER_NAME: &str = "Male Base Mesh";
