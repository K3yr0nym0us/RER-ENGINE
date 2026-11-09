//! Empaquetado del `.save` (ZIP con `manifest.json` + assets) en Rust.
//!
//! Port de `saveProjectToFile` de Electron: copia los assets referenciados por
//! el manifiesto a un directorio temporal, reescribe las rutas absolutas a
//! relativas y comprime todo.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::Value;
use zip::write::SimpleFileOptions;

const AUDIO_EXTS: [&str; 6] = ["wav", "ogg", "mp3", "flac", "aac", "m4a"];
const FONT_EXTS: [&str; 2] = ["ttf", "otf"];
const HUD_IMAGE_EXTS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];
const MODEL_SOURCE_EXTS: [&str; 3] = ["glb", "gltf", "fbx"];
const SCRIPT_FILE_PREFIX: &str = "@file:";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PackReport {
    pub files: usize,
    pub bytes: u64,
    pub packed_assets: usize,
    pub packed_scripts: usize,
}

/// Copia los assets a un staging temporal, reescribe el manifiesto y escribe el ZIP.
pub fn pack_and_write(
    manifest: &mut Value,
    extract_dir: Option<&Path>,
    save_path: &Path,
) -> Result<PackReport, String> {
    let staging = create_staging_dir()?;
    let result = (|| {
        let mut report = pack_into_staging(manifest, extract_dir, &staging)?;
        copy_rerasset_models(manifest, extract_dir, &staging)?;
        write_manifest(manifest, &staging)?;
        write_zip(&staging, save_path, &mut report)?;
        Ok(report)
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

fn create_staging_dir() -> Result<PathBuf, String> {
    let base = std::env::temp_dir();
    let unique = format!(
        "rer-save-write-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    );
    let dir = base.join(unique);
    fs::create_dir_all(&dir).map_err(|err| format!("no se pudo crear staging: {err}"))?;
    Ok(dir)
}

fn pack_into_staging(
    manifest: &mut Value,
    extract_dir: Option<&Path>,
    staging: &Path,
) -> Result<PackReport, String> {
    let mut report = PackReport::default();
    let mut copied: HashMap<String, String> = HashMap::new();
    let mut used_names: HashMap<String, u32> = HashMap::new();

    walk_strings(manifest, &mut |text| {
        if let Some(rel) = copied.get(text) {
            *text = rel.clone();
            return;
        }
        if let Some(rel) = text.strip_prefix(SCRIPT_FILE_PREFIX) {
            copy_script_ref(rel, extract_dir, staging, &mut copied, text, &mut report)
                .unwrap_or_else(|err| log::warn!("[save] script omitido: {err}"));
            return;
        }
        if !Path::new(text.as_str()).is_absolute() || is_model_source(text) {
            return;
        }
        let source = PathBuf::from(text.as_str());
        if !source.is_file() {
            log::warn!("[save] referencia sin archivo en disco: {text}");
            return;
        }
        match copy_asset(&source, staging, &mut used_names) {
            Ok(rel) => {
                report.packed_assets += 1;
                copied.insert(text.clone(), rel.clone());
                *text = rel;
            }
            Err(err) => log::warn!("[save] no se pudo copiar {text}: {err}"),
        }
    });

    Ok(report)
}

fn copy_asset(
    source: &Path,
    staging: &Path,
    used_names: &mut HashMap<String, u32>,
) -> Result<String, String> {
    let base_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("nombre inválido: {}", source.display()))?;
    let prefix = asset_prefix(source);
    let key = format!("{prefix}/{base_name}");
    let count = used_names.entry(key).or_insert(0);
    let dest_name = if *count == 0 {
        base_name.to_string()
    } else {
        unique_name(base_name, *count)
    };
    *count += 1;

    let dest_dir = staging.join(prefix);
    fs::create_dir_all(&dest_dir)
        .map_err(|err| format!("no se pudo crear {}: {err}", dest_dir.display()))?;
    fs::copy(source, dest_dir.join(&dest_name))
        .map_err(|err| format!("no se pudo copiar {}: {err}", source.display()))?;
    Ok(format!("{prefix}/{dest_name}"))
}

fn unique_name(base_name: &str, count: u32) -> String {
    let path = Path::new(base_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(base_name);
    match path.extension().and_then(|ext| ext.to_str()) {
        Some(extension) => format!("{stem}_{count}.{extension}"),
        None => format!("{stem}_{count}"),
    }
}

fn asset_prefix(source: &Path) -> &'static str {
    let extension = extension_of(source);
    if AUDIO_EXTS.contains(&extension.as_str()) {
        "sounds"
    } else if FONT_EXTS.contains(&extension.as_str()) {
        "fonts"
    } else if HUD_IMAGE_EXTS.contains(&extension.as_str()) {
        "hud-images"
    } else {
        "assets"
    }
}

fn copy_script_ref(
    rel: &str,
    extract_dir: Option<&Path>,
    staging: &Path,
    copied: &mut HashMap<String, String>,
    original: &str,
    report: &mut PackReport,
) -> Result<(), String> {
    let dest = staging.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    if copied.contains_key(original) || dest.is_file() {
        return Ok(());
    }
    let Some(extract_dir) = extract_dir else {
        return Err(format!("sin extract dir para copiar {rel}"));
    };
    let source = extract_dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    if !source.is_file() {
        return Err(format!("script no encontrado: {}", source.display()));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("no se pudo crear {parent:?}: {err}"))?;
    }
    fs::copy(&source, &dest)
        .map_err(|err| format!("no se pudo copiar {}: {err}", source.display()))?;
    copied.insert(original.to_string(), original.to_string());
    report.packed_scripts += 1;
    Ok(())
}

/// Copia los `.rerasset` de `resources.models` (el manifiesto ya los referencia
/// con ruta relativa `imported/models/<id>.rerasset`).
fn copy_rerasset_models(
    manifest: &Value,
    extract_dir: Option<&Path>,
    staging: &Path,
) -> Result<(), String> {
    let Some(models) = manifest
        .get("resources")
        .and_then(|resources| resources.get("models"))
        .and_then(Value::as_array)
    else {
        return Ok(());
    };

    for model in models {
        let Some(id) = model.get("id").and_then(Value::as_str) else {
            continue;
        };
        let rel = model
            .get("asset")
            .and_then(Value::as_str)
            .filter(|asset| !asset.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("imported/models/{id}.rerasset"));
        let Some(source) = rerasset_candidate(extract_dir, id, &rel) else {
            return Err(format!("asset no encontrado en disco: {id} ({rel})"));
        };
        let dest = staging.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("no se pudo crear {parent:?}: {err}"))?;
        }
        fs::copy(&source, &dest).map_err(|err| {
            format!(
                "no se pudo copiar {} a {}: {err}",
                source.display(),
                dest.display()
            )
        })?;
    }

    Ok(())
}

fn rerasset_candidate(extract_dir: Option<&Path>, id: &str, rel: &str) -> Option<PathBuf> {
    if let Some(extract_dir) = extract_dir {
        let candidate = extract_dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let temp_candidate = std::env::temp_dir()
        .join("rer-engine-imported")
        .join("models")
        .join(format!("{id}.rerasset"));
    temp_candidate.is_file().then_some(temp_candidate)
}

fn write_manifest(manifest: &Value, staging: &Path) -> Result<(), String> {
    let path = staging.join("manifest.json");
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|err| format!("manifest inválido: {err}"))?;
    fs::write(&path, json).map_err(|err| format!("no se pudo escribir manifest: {err}"))
}

fn write_zip(staging: &Path, save_path: &Path, report: &mut PackReport) -> Result<(), String> {
    if let Some(parent) = save_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|err| format!("no se pudo crear {}: {err}", parent.display()))?;
    }

    let file = fs::File::create(save_path)
        .map_err(|err| format!("no se pudo crear {}: {err}", save_path.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    add_entries(&mut zip, staging, staging, options, report)?;
    zip.finish()
        .map_err(|err| format!("no se pudo finalizar el ZIP: {err}"))?;
    Ok(())
}

fn add_entries(
    zip: &mut zip::ZipWriter<fs::File>,
    root: &Path,
    dir: &Path,
    options: SimpleFileOptions,
    report: &mut PackReport,
) -> Result<(), String> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|err| format!("no se pudo leer {}: {err}", dir.display()))?
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            add_entries(zip, root, &path, options, report)?;
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let name = relative.to_string_lossy().replace('\\', "/");
        zip.start_file(name, options)
            .map_err(|err| format!("no se pudo añadir {} al ZIP: {err}", path.display()))?;
        let bytes =
            fs::read(&path).map_err(|err| format!("no se pudo leer {}: {err}", path.display()))?;
        zip.write_all(&bytes)
            .map_err(|err| format!("no se pudo escribir {} en el ZIP: {err}", path.display()))?;
        report.files += 1;
        report.bytes += bytes.len() as u64;
    }

    Ok(())
}

/// Recorre todas las cadenas de un JSON y permite reescribirlas.
fn walk_strings(value: &mut Value, visit: &mut impl FnMut(&mut String)) {
    match value {
        Value::String(text) => visit(text),
        Value::Array(items) => {
            for item in items {
                walk_strings(item, visit);
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                walk_strings(item, visit);
            }
        }
        _ => {}
    }
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_lowercase()
}

fn is_model_source(path: &str) -> bool {
    let lower = path.to_lowercase();
    MODEL_SOURCE_EXTS
        .iter()
        .any(|extension| lower.ends_with(&format!(".{extension}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_model_sources() {
        assert!(is_model_source("C:/models/hero.glb"));
        assert!(is_model_source("hero.GLTF"));
        assert!(!is_model_source("C:/models/hero.rerasset"));
    }

    #[test]
    fn classifies_assets_by_extension() {
        assert_eq!(asset_prefix(Path::new("a.wav")), "sounds");
        assert_eq!(asset_prefix(Path::new("a.TTF")), "fonts");
        assert_eq!(asset_prefix(Path::new("a.png")), "hud-images");
        assert_eq!(asset_prefix(Path::new("a.bin")), "assets");
    }

    #[test]
    fn renames_duplicate_basenames() {
        assert_eq!(unique_name("hero.png", 1), "hero_1.png");
        assert_eq!(unique_name("hero", 2), "hero_2");
    }

    #[test]
    fn walks_every_string() {
        let mut value = json!({ "a": ["x", { "b": "y" }], "c": 1 });
        let mut seen = Vec::new();
        walk_strings(&mut value, &mut |text| seen.push(text.clone()));
        seen.sort();
        assert_eq!(seen, vec!["x", "y"]);
    }
}
