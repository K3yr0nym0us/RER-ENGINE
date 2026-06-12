//! Textura bakeada `.rtex` — mips finales listos para GPU.

use std::io::{Read, Write};

pub const RTEX_MAGIC: &[u8; 4] = b"RTEX";
pub const RTEX_VERSION: u16 = 1;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureFormat {
    Rgba8UnormSrgb = 0,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressionType {
    None = 0,
}

#[derive(Clone, Debug)]
pub struct RtexData {
    pub width:            u32,
    pub height:           u32,
    pub texture_format:   TextureFormat,
    pub compression_type: CompressionType,
    /// mip0 .. mipN consecutivos (RGBA8).
    pub mips:             Vec<Vec<u8>>,
}

pub fn write_rtex(w: &mut impl Write, data: &RtexData) -> std::io::Result<()> {
    w.write_all(RTEX_MAGIC)?;
    w.write_all(&RTEX_VERSION.to_le_bytes())?;
    w.write_all(&data.width.to_le_bytes())?;
    w.write_all(&data.height.to_le_bytes())?;
    let mip_count = u8::try_from(data.mips.len()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "demasiados mips")
    })?;
    w.write_all(&[mip_count])?;
    w.write_all(&[data.texture_format as u8])?;
    w.write_all(&[data.compression_type as u8])?;
    // 2 bytes reservados — read_rtex consume los 5 bytes (mip_count + format + compression + reserved[2]).
    w.write_all(&[0u8; 2])?;
    for mip in &data.mips {
        w.write_all(mip)?;
    }
    Ok(())
}

pub fn read_rtex(r: &mut impl Read) -> std::io::Result<RtexData> {
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic)?;
    if &magic != RTEX_MAGIC {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "magic RTEX inválido",
        ));
    }
    let mut ver = [0u8; 2];
    r.read_exact(&mut ver)?;
    let version = u16::from_le_bytes(ver);
    if version != RTEX_VERSION {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("versión RTEX no soportada: {version}"),
        ));
    }
    let mut u32buf = [0u8; 4];
    r.read_exact(&mut u32buf)?;
    let width = u32::from_le_bytes(u32buf);
    r.read_exact(&mut u32buf)?;
    let height = u32::from_le_bytes(u32buf);
    let mut meta = [0u8; 5];
    r.read_exact(&mut meta)?;
    let mip_count = meta[0] as usize;
    let texture_format = match meta[1] {
        0 => TextureFormat::Rgba8UnormSrgb,
        other => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("texture_format desconocido: {other}"),
            ));
        }
    };
    let compression_type = match meta[2] {
        0 => CompressionType::None,
        other => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("compression_type desconocido: {other}"),
            ));
        }
    };

    let mut mips = Vec::with_capacity(mip_count);
    let mut w = width;
    let mut h = height;
    for _ in 0..mip_count {
        let byte_len = (w as usize)
            .saturating_mul(h as usize)
            .saturating_mul(4);
        let mut buf = vec![0u8; byte_len];
        r.read_exact(&mut buf)?;
        mips.push(buf);
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }

    Ok(RtexData {
        width,
        height,
        texture_format,
        compression_type,
        mips,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtex_roundtrip() {
        let mut mip0 = vec![255u8; 64];
        mip0[0..4].copy_from_slice(&[15, 15, 15, 255]);
        let data = RtexData {
            width: 4,
            height: 4,
            texture_format: TextureFormat::Rgba8UnormSrgb,
            compression_type: CompressionType::None,
            mips: vec![mip0, vec![128u8; 16]],
        };
        let mut buf = Vec::new();
        write_rtex(&mut buf, &data).unwrap();
        let mut cursor = std::io::Cursor::new(buf);
        let back = read_rtex(&mut cursor).unwrap();
        assert_eq!(back.width, 4);
        assert_eq!(back.mips.len(), 2);
        assert_eq!(back.mips[0].len(), 64);
        assert_eq!(&back.mips[0][0..4], &[15, 15, 15, 255]);
    }
}
