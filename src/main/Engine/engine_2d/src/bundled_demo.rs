//! Demo 2D embebida en el binario (`assets/DEMO_2d.save`): plantilla por defecto.

use std::fs::{self, File};
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};

/// Bytes del `.save` (ZIP) incluidos en compile-time. Sin archivo externo en runtime.
const DEMO_SAVE_BYTES: &[u8] = include_bytes!("../assets/DEMO_2d.save");

const CACHE_DIR_NAME: &str = "rer-engine-bundled-demo-2d";
const STAMP_FILE: &str = ".demo_stamp";

fn content_stamp() -> String {
    // Longitud + FNV-1a de 64 bits: invalida caché si cambia el blob embebido.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in DEMO_SAVE_BYTES {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{}:{hash:016x}", DEMO_SAVE_BYTES.len())
}

fn cache_dir() -> Result<PathBuf, String> {
    let base = std::env::temp_dir().join(CACHE_DIR_NAME);
    fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    Ok(base)
}

fn extract_zip_bytes_to(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let cursor = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("zip embebido DEMO_2d: {e}"))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("entrada zip {i}: {e}"))?;
        let raw_name = entry.name().replace('\\', "/");
        if raw_name.is_empty() || raw_name.contains("..") {
            continue;
        }
        let out_path = dest.join(Path::new(&raw_name));
        if raw_name.ends_with('/') {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut outfile = File::create(&out_path).map_err(|e| e.to_string())?;
        io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Extrae la demo embebida a un directorio de caché (idempotente) y devuelve su ruta.
pub fn ensure_bundled_demo_2d_extract_dir() -> Result<PathBuf, String> {
    if DEMO_SAVE_BYTES.is_empty() {
        return Err("DEMO_2d embebida vacía".into());
    }
    let dest = cache_dir()?;
    let stamp_path = dest.join(STAMP_FILE);
    let manifest_path = dest.join("manifest.json");
    let expected = content_stamp();

    let cache_ok = manifest_path.is_file()
        && stamp_path.is_file()
        && fs::read_to_string(&stamp_path).ok().as_deref() == Some(expected.as_str());
    if cache_ok {
        return Ok(dest);
    }

    if dest.exists() {
        let _ = fs::remove_dir_all(&dest);
    }
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    log::info!(
        "[demo_2d] extrayendo plantilla embebida ({} bytes) → {}",
        DEMO_SAVE_BYTES.len(),
        dest.display()
    );
    extract_zip_bytes_to(DEMO_SAVE_BYTES, &dest)?;
    fs::write(&stamp_path, &expected).map_err(|e| e.to_string())?;
    Ok(dest)
}
