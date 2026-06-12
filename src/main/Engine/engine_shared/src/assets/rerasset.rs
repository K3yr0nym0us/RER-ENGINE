//! Contenedor binario `.rerasset` — Header + tabla de chunks + blobs.

use std::io::{Read, Write};

use super::mesh::{SkinnedMeshVertex, StaticMeshVertex};
use super::rtex::{read_rtex, write_rtex, RtexData};
use bytemuck::{cast_slice, Pod};

pub const RERA_MAGIC: &[u8; 4] = b"RERA";
pub const RER_ASSET_VERSION: u16 = 1;
pub const RER_IMPORTER_VERSION: u16 = 2;

pub const ASSET_HEADER_SIZE: usize = 128;
pub const CHUNK_ENTRY_SIZE: usize = 24;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetState {
    Importing = 0,
    Ready     = 1,
    Failed    = 2,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceExt {
    Glb  = 0,
    Gltf = 1,
    Fbx  = 2,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct AssetFlags: u32 {
        const HAS_PLAY_CHARACTER = 1;
        const HAS_SKINNED        = 2;
        const HAS_SHA256         = 4;
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkType {
    Texture        = 1,
    Material       = 2,
    MeshPartMeta   = 3,
    MeshEditorVert = 4,
    MeshEditorIdx  = 5,
    MeshPlayVert   = 6,
    MeshPlayIdx    = 7,
    MeshSkinnedVert = 8,
    MeshSkinnedIdx  = 9,
    Skeleton       = 10,
    AnimClip       = 11,
}

#[derive(Clone, Debug)]
pub struct AssetHeader {
    pub asset_version:     u16,
    pub importer_version:  u16,
    pub flags:             AssetFlags,
    pub category:          u8,
    pub source_ext:        SourceExt,
    pub chunk_count:       u16,
    pub material_count:    u16,
    pub clip_count:        u16,
    pub source_size:       u64,
    pub source_mtime_secs: u64,
    pub source_sha256:     [u8; 32],
}

#[derive(Clone, Debug)]
pub struct ChunkEntry {
    pub chunk_type:  ChunkType,
    pub chunk_index: u32,
    pub offset:      u64,
    pub size:        u64,
}

#[derive(Clone, Debug)]
pub struct MaterialDesc {
    pub material_index:      u32,
    pub texture_chunk_index: u32,
    pub name:                String,
}

#[derive(Clone, Debug)]
pub struct BakeMeshPart {
    pub part_index:     u16,
    pub material_index: u32,
    pub forward_xz:     [f32; 2],
    pub local_bounds:   ([f32; 3], [f32; 3]),
    pub vertices:       Vec<StaticMeshVertex>,
    pub indices:        Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct BakeSkinnedPart {
    pub part_index:     u16,
    pub name:           String,
    pub material_index: u32,
    pub vertices:       Vec<SkinnedMeshVertex>,
    pub indices:        Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct ImportedAnimationClip {
    pub name:       String,
    pub duration_s: f32,
    pub fps:        f32,
    pub blob:       Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct ImportedSkeleton {
    pub blob: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct BakeInput {
    pub category:           u8,
    pub source_ext:         SourceExt,
    pub source_size:        u64,
    pub source_mtime_secs:  u64,
    pub source_sha256:      Option<[u8; 32]>,
    pub textures:           Vec<RtexData>,
    pub materials:          Vec<MaterialDesc>,
    pub editor_parts:       Vec<BakeMeshPart>,
    pub play_parts:         Option<Vec<BakeMeshPart>>,
    pub skinned_parts:      Option<Vec<BakeSkinnedPart>>,
    pub skeleton:           Option<ImportedSkeleton>,
    pub clips:              Vec<ImportedAnimationClip>,
}

#[derive(Clone, Debug)]
pub struct RerassetFile {
    pub header:    AssetHeader,
    pub chunks:    Vec<ChunkEntry>,
    pub raw_bytes: Vec<u8>,
}

fn write_string(w: &mut impl Write, s: &str) -> std::io::Result<()> {
    let bytes = s.as_bytes();
    let len = u16::try_from(bytes.len()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "string demasiado largo")
    })?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(bytes)
}

fn write_pod_slice<T: Pod>(w: &mut impl Write, slice: &[T]) -> std::io::Result<()> {
    w.write_all(cast_slice(slice))
}

fn read_pod_vec<T: Pod>(r: &mut impl Read, byte_len: usize) -> std::io::Result<Vec<T>> {
    let elem = std::mem::size_of::<T>();
    if byte_len % elem != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "tamaño de chunk no alineado",
        ));
    }
    let count = byte_len / elem;
    let mut buf = vec![0u8; byte_len];
    r.read_exact(&mut buf)?;
    let mut out = Vec::with_capacity(count);
    for chunk in buf.chunks_exact(elem) {
        // SAFETY: T is Pod and chunk is exact size
        let val = *bytemuck::from_bytes::<T>(chunk);
        out.push(val);
    }
    Ok(out)
}

fn write_mesh_part_meta(
    w: &mut impl Write,
    part_index: u16,
    variant: u8,
    material_index: u32,
    forward_xz: [f32; 2],
    bounds: ([f32; 3], [f32; 3]),
    name: &str,
) -> std::io::Result<()> {
    w.write_all(&part_index.to_le_bytes())?;
    w.write_all(&[variant, 0])?;
    w.write_all(&material_index.to_le_bytes())?;
    for f in forward_xz {
        w.write_all(&f.to_le_bytes())?;
    }
    for f in bounds.0 {
        w.write_all(&f.to_le_bytes())?;
    }
    for f in bounds.1 {
        w.write_all(&f.to_le_bytes())?;
    }
    write_string(w, name)
}

fn write_material_chunk(w: &mut impl Write, mat: &MaterialDesc) -> std::io::Result<()> {
    w.write_all(&mat.material_index.to_le_bytes())?;
    w.write_all(&mat.texture_chunk_index.to_le_bytes())?;
    write_string(w, &mat.name)
}

pub fn write_rerasset(w: &mut impl Write, input: &BakeInput) -> std::io::Result<()> {
    let mut flags = AssetFlags::empty();
    if input.play_parts.is_some() {
        flags |= AssetFlags::HAS_PLAY_CHARACTER;
    }
    if input.skinned_parts.is_some() {
        flags |= AssetFlags::HAS_SKINNED;
    }
    let mut sha = [0u8; 32];
    if let Some(h) = input.source_sha256 {
        sha = h;
        flags |= AssetFlags::HAS_SHA256;
    }

    let mut chunk_bodies: Vec<(ChunkType, u32, Vec<u8>)> = Vec::new();
    for (ti, tex) in input.textures.iter().enumerate() {
        let mut body = Vec::new();
        write_rtex(&mut body, tex)?;
        chunk_bodies.push((ChunkType::Texture, ti as u32, body));
    }

    for mat in &input.materials {
        let mut body = Vec::new();
        write_material_chunk(&mut body, mat)?;
        chunk_bodies.push((ChunkType::Material, mat.material_index, body));
    }

    for part in &input.editor_parts {
        let mut meta = Vec::new();
        write_mesh_part_meta(
            &mut meta,
            part.part_index,
            0,
            part.material_index,
            part.forward_xz,
            part.local_bounds,
            "",
        )?;
        chunk_bodies.push((ChunkType::MeshPartMeta, part.part_index as u32, meta));

        let mut vert = Vec::new();
        write_pod_slice(&mut vert, &part.vertices)?;
        chunk_bodies.push((ChunkType::MeshEditorVert, part.part_index as u32, vert));

        let mut idx = Vec::new();
        for i in &part.indices {
            idx.extend_from_slice(&i.to_le_bytes());
        }
        chunk_bodies.push((ChunkType::MeshEditorIdx, part.part_index as u32, idx));
    }

    if let Some(play_parts) = &input.play_parts {
        for part in play_parts {
            let mut meta = Vec::new();
            write_mesh_part_meta(
                &mut meta,
                part.part_index,
                1,
                part.material_index,
                part.forward_xz,
                part.local_bounds,
                "",
            )?;
            chunk_bodies.push((ChunkType::MeshPartMeta, part.part_index as u32, meta));

            let mut vert = Vec::new();
            write_pod_slice(&mut vert, &part.vertices)?;
            chunk_bodies.push((ChunkType::MeshPlayVert, part.part_index as u32, vert));

            let mut idx = Vec::new();
            for i in &part.indices {
                idx.extend_from_slice(&i.to_le_bytes());
            }
            chunk_bodies.push((ChunkType::MeshPlayIdx, part.part_index as u32, idx));
        }
    }

    if let Some(skinned) = &input.skinned_parts {
        for part in skinned {
            let mut meta = Vec::new();
            write_mesh_part_meta(
                &mut meta,
                part.part_index,
                2,
                part.material_index,
                [0.0, 1.0],
                ([0.0; 3], [0.0; 3]),
                &part.name,
            )?;
            chunk_bodies.push((ChunkType::MeshPartMeta, part.part_index as u32, meta));

            let mut vert = Vec::new();
            write_pod_slice(&mut vert, &part.vertices)?;
            chunk_bodies.push((ChunkType::MeshSkinnedVert, part.part_index as u32, vert));

            let mut idx = Vec::new();
            for i in &part.indices {
                idx.extend_from_slice(&i.to_le_bytes());
            }
            chunk_bodies.push((ChunkType::MeshSkinnedIdx, part.part_index as u32, idx));
        }
    }

    if let Some(sk) = &input.skeleton {
        chunk_bodies.push((ChunkType::Skeleton, 0, sk.blob.clone()));
    }

    for (ci, clip) in input.clips.iter().enumerate() {
        chunk_bodies.push((ChunkType::AnimClip, ci as u32, clip.blob.clone()));
    }

    let chunk_count = u16::try_from(chunk_bodies.len()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "demasiados chunks")
    })?;

    let header = AssetHeader {
        asset_version: RER_ASSET_VERSION,
        importer_version: RER_IMPORTER_VERSION,
        flags,
        category: input.category,
        source_ext: input.source_ext,
        chunk_count,
        material_count: input.materials.len() as u16,
        clip_count: input.clips.len() as u16,
        source_size: input.source_size,
        source_mtime_secs: input.source_mtime_secs,
        source_sha256: sha,
    };

    let directory_offset = ASSET_HEADER_SIZE;
    let data_offset = directory_offset + chunk_bodies.len() * CHUNK_ENTRY_SIZE;

    let mut offset = data_offset as u64;
    let mut directory: Vec<ChunkEntry> = Vec::with_capacity(chunk_bodies.len());
    let mut data_blob = Vec::new();
    for (ctype, cindex, body) in &chunk_bodies {
        directory.push(ChunkEntry {
            chunk_type: *ctype,
            chunk_index: *cindex,
            offset,
            size: body.len() as u64,
        });
        data_blob.extend_from_slice(body);
        offset += body.len() as u64;
    }

    w.write_all(RERA_MAGIC)?;
    w.write_all(&header.asset_version.to_le_bytes())?;
    w.write_all(&header.importer_version.to_le_bytes())?;
    w.write_all(&header.flags.bits().to_le_bytes())?;
    w.write_all(&[header.category])?;
    w.write_all(&[header.source_ext as u8])?;
    w.write_all(&[0u8; 2])?;
    w.write_all(&header.chunk_count.to_le_bytes())?;
    w.write_all(&header.material_count.to_le_bytes())?;
    w.write_all(&header.clip_count.to_le_bytes())?;
    w.write_all(&[0u8; 2])?; // align → offset 24
    w.write_all(&header.source_size.to_le_bytes())?;
    w.write_all(&header.source_mtime_secs.to_le_bytes())?;
    w.write_all(&header.source_sha256)?;
    w.write_all(&[0u8; 56])?; // reserved → header 128 bytes

    for entry in &directory {
        w.write_all(&(entry.chunk_type as u32).to_le_bytes())?;
        w.write_all(&entry.chunk_index.to_le_bytes())?;
        w.write_all(&entry.offset.to_le_bytes())?;
        w.write_all(&entry.size.to_le_bytes())?;
    }

    w.write_all(&data_blob)?;
    Ok(())
}

pub fn read_rerasset(bytes: &[u8]) -> std::io::Result<RerassetFile> {
    if bytes.len() < ASSET_HEADER_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "archivo demasiado corto",
        ));
    }
    if &bytes[0..4] != RERA_MAGIC {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "magic RERA inválido",
        ));
    }
    let asset_version = u16::from_le_bytes([bytes[4], bytes[5]]);
    let importer_version = u16::from_le_bytes([bytes[6], bytes[7]]);
    let flags = AssetFlags::from_bits_truncate(u32::from_le_bytes([
        bytes[8], bytes[9], bytes[10], bytes[11],
    ]));
    let category = bytes[12];
    let source_ext = match bytes[13] {
        0 => SourceExt::Glb,
        1 => SourceExt::Gltf,
        2 => SourceExt::Fbx,
        other => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("source_ext desconocido: {other}"),
            ));
        }
    };
    let chunk_count = u16::from_le_bytes([bytes[16], bytes[17]]) as usize;
    let material_count = u16::from_le_bytes([bytes[18], bytes[19]]);
    let clip_count = u16::from_le_bytes([bytes[20], bytes[21]]);
    let source_size = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
    let source_mtime_secs = u64::from_le_bytes(bytes[32..40].try_into().unwrap());
    let mut source_sha256 = [0u8; 32];
    source_sha256.copy_from_slice(&bytes[40..72]);

    let header = AssetHeader {
        asset_version,
        importer_version,
        flags,
        category,
        source_ext,
        chunk_count: chunk_count as u16,
        material_count,
        clip_count,
        source_size,
        source_mtime_secs,
        source_sha256,
    };

    let dir_start = ASSET_HEADER_SIZE;
    let dir_end = dir_start + chunk_count * CHUNK_ENTRY_SIZE;
    if bytes.len() < dir_end {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "tabla de chunks truncada",
        ));
    }

    let mut chunks = Vec::with_capacity(chunk_count);
    for i in 0..chunk_count {
        let base = dir_start + i * CHUNK_ENTRY_SIZE;
        let ctype_raw = u32::from_le_bytes(bytes[base..base + 4].try_into().unwrap());
        let chunk_type = match ctype_raw {
            1 => ChunkType::Texture,
            2 => ChunkType::Material,
            3 => ChunkType::MeshPartMeta,
            4 => ChunkType::MeshEditorVert,
            5 => ChunkType::MeshEditorIdx,
            6 => ChunkType::MeshPlayVert,
            7 => ChunkType::MeshPlayIdx,
            8 => ChunkType::MeshSkinnedVert,
            9 => ChunkType::MeshSkinnedIdx,
            10 => ChunkType::Skeleton,
            11 => ChunkType::AnimClip,
            other => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("chunk_type desconocido: {other}"),
                ));
            }
        };
        let chunk_index = u32::from_le_bytes(bytes[base + 4..base + 8].try_into().unwrap());
        let offset = u64::from_le_bytes(bytes[base + 8..base + 16].try_into().unwrap());
        let size = u64::from_le_bytes(bytes[base + 16..base + 24].try_into().unwrap());
        chunks.push(ChunkEntry {
            chunk_type,
            chunk_index,
            offset,
            size,
        });
    }

    Ok(RerassetFile {
        header,
        chunks,
        raw_bytes: bytes.to_vec(),
    })
}

impl RerassetFile {
    pub fn chunk_data(&self, entry: &ChunkEntry) -> std::io::Result<&[u8]> {
        let start = entry.offset as usize;
        let end = start + entry.size as usize;
        self.raw_bytes
            .get(start..end)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "chunk fuera de rango"))
    }

    pub fn read_texture(&self, entry: &ChunkEntry) -> std::io::Result<RtexData> {
        let data = self.chunk_data(entry)?;
        read_rtex(&mut std::io::Cursor::new(data))
    }

    pub fn read_editor_vertices(&self, entry: &ChunkEntry) -> std::io::Result<Vec<StaticMeshVertex>> {
        let data = self.chunk_data(entry)?;
        read_pod_vec(&mut std::io::Cursor::new(data), data.len())
    }

    pub fn read_skinned_vertices(
        &self,
        entry: &ChunkEntry,
    ) -> std::io::Result<Vec<super::mesh::SkinnedMeshVertex>> {
        let data = self.chunk_data(entry)?;
        read_pod_vec(&mut std::io::Cursor::new(data), data.len())
    }

    pub fn read_indices(&self, entry: &ChunkEntry) -> std::io::Result<Vec<u32>> {
        let data = self.chunk_data(entry)?;
        if data.len() % 4 != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "índices mal alineados",
            ));
        }
        let mut out = Vec::with_capacity(data.len() / 4);
        for chunk in data.chunks_exact(4) {
            out.push(u32::from_le_bytes(chunk.try_into().unwrap()));
        }
        Ok(out)
    }

    pub fn chunks_of_type(&self, ty: ChunkType) -> Vec<&ChunkEntry> {
        self.chunks.iter().filter(|c| c.chunk_type == ty).collect()
    }
}

/// Escritura atómica: `.tmp` → rename.
pub fn write_rerasset_atomic(path: &std::path::Path, input: &BakeInput) -> std::io::Result<()> {
    let tmp = path.with_extension("rerasset.tmp");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    {
        let mut f = std::fs::File::create(&tmp)?;
        write_rerasset(&mut f, input)?;
        f.sync_all()?;
    }
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{CompressionType, TextureFormat};

    #[test]
    fn rerasset_minimal_roundtrip() {
        let tex = RtexData {
            width: 2,
            height: 2,
            texture_format: TextureFormat::Rgba8UnormSrgb,
            compression_type: CompressionType::None,
            mips: vec![vec![255u8; 16]],
        };
        let input = BakeInput {
            category: 2,
            source_ext: SourceExt::Glb,
            source_size: 1024,
            source_mtime_secs: 1_700_000_000,
            source_sha256: None,
            textures: vec![tex],
            materials: vec![MaterialDesc {
                material_index: 0,
                texture_chunk_index: 0,
                name: "mat0".into(),
            }],
            editor_parts: vec![BakeMeshPart {
                part_index: 0,
                material_index: 0,
                forward_xz: [0.0, 1.0],
                local_bounds: ([-1.0, 0.0, -1.0], [1.0, 2.0, 1.0]),
                vertices: vec![StaticMeshVertex {
                    position: [0.0, 0.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 0.0],
                }],
                indices: vec![0],
            }],
            play_parts: None,
            skinned_parts: None,
            skeleton: None,
            clips: vec![],
        };
        let mut buf = Vec::new();
        write_rerasset(&mut buf, &input).unwrap();
        let file = read_rerasset(&buf).unwrap();
        assert_eq!(file.header.importer_version, RER_IMPORTER_VERSION);
        assert!(!file.chunks_of_type(ChunkType::MeshEditorVert).is_empty());
    }
}
