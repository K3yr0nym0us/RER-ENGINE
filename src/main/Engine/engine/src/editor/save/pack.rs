//! Empaquetado del `.save` (ZIP con `manifest.json` + assets) en Rust.
//!
//! Port de `saveProjectToFile` de Electron: copia los assets referenciados por
//! el manifiesto a un directorio temporal, reescribe las rutas absolutas a
//! relativas y comprime todo.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use rer_engine_core::i18n::I18nConfig;
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
    let i18n = validate_pack_manifest(manifest)?;
    let staging = create_staging_dir()?;
    let result = (|| {
        let mut report = pack_into_staging(manifest, extract_dir, &staging)?;
        let relative_catalogs =
            copy_catalogs(&i18n.catalog_paths, extract_dir, &staging, &mut report)?;
        rewrite_catalog_paths(manifest, relative_catalogs);
        copy_rerasset_models(manifest, extract_dir, &staging)?;
        write_manifest(manifest, &staging)?;
        write_zip(&staging, save_path, &mut report)?;
        Ok(report)
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

fn validate_pack_manifest(manifest: &Value) -> Result<I18nConfig, String> {
    if manifest.get("version").is_some() || manifest.get("language").is_some() {
        return Err(
            "El manifiesto es legacy: contiene 'version' o 'language' y no es compatible"
                .to_string(),
        );
    }
    let config = manifest
        .get("i18n")
        .ok_or_else(|| "El manifiesto no contiene 'i18n'".to_string())?;
    serde_json::from_value::<I18nConfig>(config.clone())
        .map_err(|error| format!("configuración i18n inválida: {error}"))
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

    walk_manifest_strings(manifest, &mut |text| {
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

fn copy_catalogs(
    catalog_paths: &[PathBuf],
    extract_dir: Option<&Path>,
    staging: &Path,
    report: &mut PackReport,
) -> Result<Vec<String>, String> {
    let mut relative_paths = Vec::with_capacity(catalog_paths.len());
    if catalog_paths.is_empty() {
        return Ok(relative_paths);
    }

    let destination_dir = staging.join("i18n");
    fs::create_dir_all(&destination_dir)
        .map_err(|error| format!("no se pudo crear {}: {error}", destination_dir.display()))?;
    let mut used_names = HashMap::new();

    for path in catalog_paths {
        let source = catalog_source(path, extract_dir)?;
        if !source.is_file() {
            return Err(format!("catálogo no encontrado: {}", source.display()));
        }
        let base_name = safe_catalog_name(path)?;
        let key = format!("i18n/{base_name}");
        let count = used_names.entry(key).or_insert(0);
        let destination_name = if *count == 0 {
            base_name.clone()
        } else {
            unique_name(&base_name, *count)
        };
        *count += 1;

        let destination = destination_dir.join(&destination_name);
        fs::copy(&source, &destination).map_err(|error| {
            format!(
                "no se pudo copiar el catálogo {} a {}: {error}",
                source.display(),
                destination.display()
            )
        })?;
        relative_paths.push(format!("i18n/{destination_name}"));
        report.packed_assets += 1;
    }

    Ok(relative_paths)
}

fn catalog_source(path: &Path, extract_dir: Option<&Path>) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let Some(extract_dir) = extract_dir else {
        return Ok(path.to_path_buf());
    };

    let root = extract_dir
        .canonicalize()
        .map_err(|error| format!("no se pudo resolver {}: {error}", extract_dir.display()))?;
    let relative = path.to_string_lossy().replace('\\', "/");
    let source = root
        .join(relative)
        .canonicalize()
        .map_err(|error| format!("catálogo no encontrado: {}: {error}", path.display()))?;
    if !source.starts_with(&root) {
        return Err(format!(
            "la ruta del catálogo sale del .save: {}",
            path.display()
        ));
    }
    Ok(source)
}

fn safe_catalog_name(path: &Path) -> Result<String, String> {
    let fallback = path
        .to_string_lossy()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_string();
    let raw = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&fallback);
    let mut name: String = raw
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect();
    name = name.trim_end_matches([' ', '.']).to_string();
    if name.is_empty() {
        return Err(format!("nombre de catálogo inválido: {}", path.display()));
    }

    let stem = name.split('.').next().unwrap_or(&name).to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        name.insert(0, '_');
    }
    Ok(name)
}

fn rewrite_catalog_paths(manifest: &mut Value, relative_paths: Vec<String>) {
    if let Some(i18n) = manifest.get_mut("i18n").and_then(Value::as_object_mut) {
        i18n.insert(
            "catalogPaths".into(),
            Value::Array(relative_paths.into_iter().map(Value::String).collect()),
        );
    }
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
        // Modelo globalizado o promocionado: el `.rerasset` sigue en
        // `scene-{id}/assets/` del `.save` cargado y el manifiesto ya lo enruta a
        // `imported/models/` (biblioteca global).
        if rel.starts_with("imported/models/")
            && let Some(found) = rerasset_in_scene_dirs(extract_dir, id)
        {
            return Some(found);
        }
        // Fallback: saves antiguos guardaban los modelos solo en la biblioteca
        // global (`imported/models/<id>.rerasset`), incluso si el manifiesto
        // nuevo los enruta por escena (`scene-{id}/assets/…`).
        let global_candidate = extract_dir
            .join("imported")
            .join("models")
            .join(format!("{id}.rerasset"));
        if global_candidate.is_file() {
            return Some(global_candidate);
        }
    }
    let temp_candidate = std::env::temp_dir()
        .join("rer-engine-imported")
        .join("models")
        .join(format!("{id}.rerasset"));
    temp_candidate.is_file().then_some(temp_candidate)
}

/// Busca `scene-{id}/assets/<id>.rerasset` dentro del `.save` extraído.
fn rerasset_in_scene_dirs(extract_dir: &Path, id: &str) -> Option<PathBuf> {
    let mut scenes: Vec<PathBuf> = fs::read_dir(extract_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("scene-"))
        .map(|entry| entry.path())
        .collect();
    scenes.sort();
    scenes
        .into_iter()
        .map(|dir| dir.join("assets").join(format!("{id}.rerasset")))
        .find(|candidate| candidate.is_file())
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

/// Los catálogos se copian de forma dedicada bajo `i18n/`; sus rutas no deben
/// pasar por el recorrido de assets genéricos, que solo acepta rutas absolutas.
fn walk_manifest_strings(value: &mut Value, visit: &mut impl FnMut(&mut String)) {
    if let Some(root) = value.as_object_mut() {
        for (key, item) in root {
            if key != "i18n" {
                walk_strings(item, visit);
            }
        }
    } else {
        walk_strings(value, visit);
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

    #[test]
    fn validate_pack_manifest_returns_i18n_config() {
        let config = validate_pack_manifest(&json!({
            "type": "2D",
            "i18n": {
                "defaultLanguage": "es",
                "fallbackLanguage": "en",
                "catalogPaths": ["i18n/ui.json"]
            }
        }))
        .unwrap();
        assert_eq!(config.default_language.to_string(), "es");
        assert_eq!(config.fallback_language.to_string(), "en");
        assert_eq!(config.catalog_paths, vec![PathBuf::from("i18n/ui.json")]);
    }

    #[test]
    fn validate_pack_manifest_rejects_missing_legacy_or_invalid_i18n() {
        for (manifest, expected) in [
            (json!({ "type": "2D" }), "i18n"),
            (
                json!({
                    "type": "2D",
                    "version": 1,
                    "i18n": { "defaultLanguage": "en", "fallbackLanguage": "en" }
                }),
                "legacy",
            ),
            (
                json!({
                    "type": "2D",
                    "language": "es",
                    "i18n": { "defaultLanguage": "es", "fallbackLanguage": "en" }
                }),
                "legacy",
            ),
            (
                json!({
                    "type": "2D",
                    "i18n": {
                        "defaultLanguage": "en",
                        "fallbackLanguage": "en",
                        "fallbackLangauge": "en"
                    }
                }),
                "i18n inválida",
            ),
        ] {
            let error = validate_pack_manifest(&manifest).expect_err("manifiesto incompatible");
            assert!(error.contains(expected), "{error}");
        }
    }

    #[test]
    fn manifest_asset_walk_skips_i18n_subtree() {
        let absolute = "C:/catalogs/ui.json";
        let mut value = json!({
            "sounds": [{ "path": absolute }],
            "i18n": { "catalogPaths": [absolute] }
        });
        let mut seen = Vec::new();
        walk_manifest_strings(&mut value, &mut |text| seen.push(text.clone()));
        assert_eq!(seen, vec![absolute]);
    }

    #[test]
    fn catalogs_use_safe_unique_names() {
        assert_eq!(
            safe_catalog_name(Path::new("../../ui.json")).unwrap(),
            "ui.json"
        );
        assert_eq!(
            safe_catalog_name(Path::new("CON.json")).unwrap(),
            "_CON.json"
        );
        assert!(safe_catalog_name(Path::new("..")).is_err());

        let dir = std::env::temp_dir().join(format!(
            "rer-pack-i18n-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let first = dir.join("first");
        let second = dir.join("second");
        fs::create_dir_all(first.join("nested")).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(first.join("ui.json"), b"first").unwrap();
        fs::write(second.join("ui.json"), b"second").unwrap();
        let staging = dir.join("staging");
        fs::create_dir_all(&staging).unwrap();
        let mut report = PackReport::default();

        let paths = copy_catalogs(
            &[
                first.join("nested").join("..").join("ui.json"),
                second.join("ui.json"),
            ],
            None,
            &staging,
            &mut report,
        )
        .unwrap();

        assert_eq!(paths, vec!["i18n/ui.json", "i18n/ui_1.json"]);
        assert_eq!(fs::read(staging.join("i18n/ui.json")).unwrap(), b"first");
        assert_eq!(fs::read(staging.join("i18n/ui_1.json")).unwrap(), b"second");
        assert_eq!(report.packed_assets, 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rerasset_candidate_finds_globalized_model_in_scene_dir() {
        // Modelo globalizado: el `.rerasset` vive en `scene-2/assets/` y el
        // manifiesto ya lo referencia en `imported/models/`.
        let dir = std::env::temp_dir().join(format!("rer-pack-globalize-{}", std::process::id()));
        let scene_dir = dir.join("scene-2").join("assets");
        fs::create_dir_all(&scene_dir).unwrap();
        let scene_asset = scene_dir.join("model_tree.rerasset");
        fs::write(&scene_asset, b"rerasset").unwrap();

        let candidate = rerasset_candidate(
            Some(&dir),
            "model_tree",
            "imported/models/model_tree.rerasset",
        );
        assert_eq!(candidate, Some(scene_asset));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rerasset_candidate_falls_back_to_global_library() {
        let dir = std::env::temp_dir().join(format!("rer-pack-test-{}", std::process::id()));
        fs::create_dir_all(dir.join("imported").join("models")).unwrap();
        let global = dir
            .join("imported")
            .join("models")
            .join("model_old.rerasset");
        fs::write(&global, b"rerasset").unwrap();

        // El manifiesto enruta por escena, pero el .save viejo solo tiene la
        // biblioteca global: el fallback la encuentra.
        let candidate =
            rerasset_candidate(Some(&dir), "model_old", "scene-2/assets/model_old.rerasset");
        assert_eq!(candidate, Some(global));

        fs::remove_dir_all(&dir).unwrap();
    }
}
