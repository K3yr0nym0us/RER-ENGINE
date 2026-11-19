//! Apertura de `.save` en Rust: extrae el ZIP y decide el tipo de proyecto.
//!
//! Electron solo entrega la ruta del archivo. Todo lo demás (descomprimir,
//! leer `manifest.json` y elegir 2D/3D) ocurre aquí, en el editor.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::editor::context::{GameStyle, ProjectType};

/// Resultado de abrir un `.save`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedSave {
    /// Carpeta temporal con `manifest.json` + assets extraídos.
    pub extract_dir: PathBuf,
    pub project_type: ProjectType,
    pub game_style: Option<GameStyle>,
}

/// Extrae `save_path` a un directorio temporal y lee `type` / `gameStyle`
/// del `manifest.json` para decidir con qué motor arranca el editor.
pub fn open_save(save_path: &Path) -> Result<OpenedSave, String> {
    let file = std::fs::File::open(save_path)
        .map_err(|error| format!("No se pudo abrir '{}': {error}", save_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("'{}' no es un .save válido: {error}", save_path.display()))?;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("rer-save-open-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("No se pudo crear el directorio temporal: {error}"))?;

    if let Err(error) = archive.extract(&dir) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(format!("No se pudo extraer el .save: {error}"));
    }

    let manifest_path = dir.join("manifest.json");
    let raw = match std::fs::read_to_string(&manifest_path) {
        Ok(raw) => raw,
        Err(_) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err("El .save no contiene manifest.json".to_string());
        }
    };
    let manifest: Value = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(format!("manifest.json inválido: {error}"));
        }
    };

    let project_type = manifest
        .get("type")
        .and_then(Value::as_str)
        .and_then(ProjectType::parse)
        .ok_or_else(|| {
            let _ = std::fs::remove_dir_all(&dir);
            "El manifest no tiene un 'type' válido (2D o 3D)".to_string()
        })?;
    let game_style = manifest
        .get("gameStyle")
        .and_then(Value::as_str)
        .and_then(GameStyle::parse);

    Ok(OpenedSave {
        extract_dir: dir,
        project_type,
        game_style,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn write_save(dir: &Path, name: &str, manifest: &str) -> PathBuf {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).expect("crear .save de prueba");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("manifest.json", options)
            .expect("escribir manifest en el ZIP");
        zip.write_all(manifest.as_bytes())
            .expect("escribir contenido del manifest");
        zip.finish().expect("cerrar el ZIP");
        path
    }

    #[test]
    fn rejects_a_missing_save_file() {
        let error = open_save(Path::new("C:/no-existe/rer-test.save"))
            .expect_err("un archivo ausente debe fallar");
        assert!(error.contains("No se pudo abrir"));
    }

    #[test]
    fn extracts_and_decides_project_type_from_manifest() {
        let base = std::env::temp_dir().join(format!("rer-save-open-test-{}", std::process::id()));
        std::fs::create_dir_all(&base).expect("crear directorio de prueba");

        for (name, manifest, expected, style) in [
            (
                "two_d.save",
                r#"{"type":"2D","gameStyle":"top-down"}"#,
                ProjectType::TwoD,
                Some(GameStyle::TopDown),
            ),
            (
                "three_d.save",
                r#"{"type":"3D","gameStyle":"first-person"}"#,
                ProjectType::ThreeD,
                Some(GameStyle::FirstPerson),
            ),
        ] {
            let save = write_save(&base, name, manifest);
            let opened = open_save(&save).expect("abrir .save de prueba");
            assert_eq!(opened.project_type, expected);
            assert_eq!(opened.game_style, style);
            assert!(opened.extract_dir.join("manifest.json").is_file());
            let _ = std::fs::remove_dir_all(&opened.extract_dir);
        }

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn rejects_a_save_without_manifest() {
        let base = std::env::temp_dir().join(format!("rer-save-open-bad-{}", std::process::id()));
        std::fs::create_dir_all(&base).expect("crear directorio de prueba");
        let path = base.join("empty.save");
        let zip = zip::ZipWriter::new(std::fs::File::create(&path).expect("crear ZIP"));
        zip.finish().expect("cerrar ZIP vacío");

        let error = open_save(&path).expect_err("sin manifest debe fallar");
        assert!(error.contains("manifest.json"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
