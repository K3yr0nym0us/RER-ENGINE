// Compatibilidad mínima de tipos base usados por el engine_2d.

#[path = "camera_base.rs"]
pub(crate) mod camera;
pub(crate) use camera::Camera;

#[path = "mesh_base.rs"]
pub(crate) mod mesh;
#[path = "physics.rs"]
pub(crate) mod physics;
