//! Pipeline de assets importados (.rerasset) en el motor 3D.

pub mod bake;
pub mod import;
pub mod load;
pub mod log_tex;
pub mod model_asset_blob;
pub mod registry;

pub use registry::{ImportedModelEntry, ImportedModelRegistry};
