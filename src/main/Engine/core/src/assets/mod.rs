//! Formatos de assets importados (.rerasset, .rtex) — sin dependencia GPU.

mod mesh;
mod rerasset;
mod rtex;

pub use mesh::{SkinnedMeshVertex, StaticMeshVertex};
pub use rerasset::{
    AssetFlags, AssetHeader, AssetState, BakeInput, BakeMeshPart, BakeSkinnedPart, ChunkEntry,
    ChunkType, ImportedAnimationClip, ImportedSkeleton, MaterialDesc, RER_ASSET_VERSION,
    RER_IMPORTER_VERSION, RerassetFile, SourceExt, read_rerasset, write_rerasset,
    write_rerasset_atomic,
};
pub use rtex::{CompressionType, RtexData, TextureFormat, read_rtex, write_rtex};
