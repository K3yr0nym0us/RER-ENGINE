//! Parseo de los argumentos con los que Electron lanza el editor.

use std::path::PathBuf;

use crate::editor::context::{ProjectContext, ProjectType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    Es,
}

impl Locale {
    pub fn parse(value: &str) -> Self {
        if value.trim().eq_ignore_ascii_case("es") {
            Self::Es
        } else {
            Self::En
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Es => "es",
        }
    }
}

/// Configuración de arranque del editor egui.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cli {
    pub context: ProjectContext,
    pub save_path: Option<String>,
    pub extract_dir: Option<String>,
    pub locale: Locale,
    /// Lanzado por Electron: el cierre pide confirmación antes de salir.
    pub electron_host: bool,
    /// Carpeta temporal creada por Rust al abrir un `.save`; se borra al salir.
    pub temp_extract_dir: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            context: ProjectContext::new(ProjectType::ThreeD, None),
            save_path: None,
            extract_dir: None,
            locale: Locale::En,
            electron_host: false,
            temp_extract_dir: None,
        }
    }
}

impl Cli {
    /// Parsea `args` (incluyendo el nombre del ejecutable en la posición 0).
    ///
    /// Es tolerante: las banderas desconocidas se ignoran y una bandera sin
    /// valor conserva el valor por defecto.
    pub fn parse_from<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let args: Vec<String> = args
            .into_iter()
            .map(|value| value.as_ref().to_string())
            .collect();

        let mut project_type = ProjectType::ThreeD;
        let mut game_style: Option<String> = None;
        let mut save_path: Option<String> = None;
        let mut extract_dir: Option<String> = None;
        let mut locale = Locale::En;
        let mut electron_host = false;

        let mut index = 1;
        while index < args.len() {
            let flag = args[index].as_str();
            let value = args
                .get(index + 1)
                .map(String::as_str)
                .filter(|value| !value.starts_with("--"));

            match flag {
                "--project-type" => {
                    if let Some(value) = value {
                        project_type = ProjectType::parse(value).unwrap_or(project_type);
                        index += 1;
                    }
                }
                "--game-style" => {
                    if let Some(value) = value {
                        let trimmed = value.trim();
                        game_style = (!trimmed.is_empty()).then(|| trimmed.to_string());
                        index += 1;
                    }
                }
                "--save-path" => {
                    if let Some(value) = value {
                        let trimmed = value.trim();
                        save_path = (!trimmed.is_empty()).then(|| trimmed.to_string());
                        index += 1;
                    }
                }
                "--extract-dir" => {
                    if let Some(value) = value {
                        let trimmed = value.trim();
                        extract_dir = (!trimmed.is_empty()).then(|| trimmed.to_string());
                        index += 1;
                    }
                }
                "--locale" => {
                    if let Some(value) = value {
                        locale = Locale::parse(value);
                        index += 1;
                    }
                }
                "--electron-host" => electron_host = true,
                _ => {}
            }

            index += 1;
        }

        Self {
            context: ProjectContext::new(project_type, game_style.as_deref()),
            save_path,
            extract_dir,
            locale,
            electron_host,
            temp_extract_dir: None,
        }
    }

    /// Si Electron entregó solo un `--save-path`, extrae el archivo en Rust y
    /// decide el tipo de proyecto leyendo su `manifest.json`.
    ///
    /// Los proyectos nuevos siguen pasando `--project-type` y no llegan aquí.
    pub fn resolve_save(&mut self) -> Result<(), String> {
        let Some(save_path) = self.save_path.clone() else {
            return Ok(());
        };
        if self.extract_dir.is_some() {
            // Ruta legacy: alguien ya extrajo el save; respetar la carpeta.
            return Ok(());
        }
        let opened = crate::editor::save_open::open_save(std::path::Path::new(&save_path))?;
        let game_style = opened.game_style.map(|style| style.as_str().to_string());
        self.context = ProjectContext::new(opened.project_type, game_style.as_deref());
        self.extract_dir = Some(opened.extract_dir.to_string_lossy().into_owned());
        self.temp_extract_dir = Some(opened.extract_dir);
        Ok(())
    }

    pub fn usage() -> &'static str {
        "rer_engine_editor [--project-type <2D|3D>] [--game-style <estilo>] \
[--save-path <ruta>] [--extract-dir <ruta>] [--locale <en|es>] [--electron-host]\n\
Con solo --save-path, el editor extrae el archivo y decide 2D/3D desde su manifest."
    }
}

/// Comando `set_scene` que el editor debe disparar al motor al abrir un `.save`.
///
/// `setup_empty_3d()` deja el mundo vacío esperando este comando; antes lo
/// enviaba Electron tras `ready`, pero ahora es el editor quien lo produce.
pub fn saved_scene_command(cli: &Cli) -> Option<serde_json::Value> {
    if cli.save_path.is_none() {
        return None;
    }
    let extract_dir = cli.extract_dir.as_deref()?;
    let scene = if cli.context.is_3d() {
        cli.context.game_style.as_str().to_string()
    } else {
        "2D".to_string()
    };
    Some(serde_json::json!({
        "cmd": "set_scene",
        "scene": scene,
        "save_path": extract_dir,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::context::GameStyle;

    #[test]
    fn parses_full_command_line() {
        let cli = Cli::parse_from([
            "rer_engine_editor",
            "--project-type",
            "2D",
            "--game-style",
            "side-scroller",
            "--save-path",
            "C:/projects/game.save",
            "--extract-dir",
            "C:/tmp/game",
            "--locale",
            "es",
        ]);

        assert_eq!(cli.context.project_type, ProjectType::TwoD);
        assert_eq!(cli.context.game_style, GameStyle::SideScroller);
        assert!(!cli.context.game_style_adjusted);
        assert_eq!(cli.save_path.as_deref(), Some("C:/projects/game.save"));
        assert_eq!(cli.extract_dir.as_deref(), Some("C:/tmp/game"));
        assert_eq!(cli.locale, Locale::Es);
    }

    #[test]
    fn uses_defaults_without_arguments() {
        let cli = Cli::parse_from(["rer_engine_editor"]);
        assert_eq!(cli, Cli::default());
    }

    #[test]
    fn ignores_unknown_flags_and_missing_values() {
        let cli = Cli::parse_from([
            "rer_engine_editor",
            "--unknown",
            "value",
            "--project-type",
            "--locale",
            "es",
        ]);

        assert_eq!(cli.context.project_type, ProjectType::ThreeD);
        assert_eq!(cli.locale, Locale::Es);
    }

    #[test]
    fn treats_blank_paths_as_absent() {
        let cli = Cli::parse_from([
            "rer_engine_editor",
            "--save-path",
            "   ",
            "--extract-dir",
            "",
            "--game-style",
            " ",
        ]);

        assert!(cli.save_path.is_none());
        assert!(cli.extract_dir.is_none());
        assert!(!cli.context.game_style_adjusted);
    }

    #[test]
    fn normalizes_invalid_style_for_project_type() {
        let cli = Cli::parse_from([
            "rer_engine_editor",
            "--project-type",
            "2D",
            "--game-style",
            "first-person",
        ]);

        assert_eq!(cli.context.game_style, GameStyle::TopDown);
        assert!(cli.context.game_style_adjusted);
    }

    #[test]
    fn locale_is_english_by_default() {
        assert_eq!(Locale::parse("en"), Locale::En);
        assert_eq!(Locale::parse("ES"), Locale::Es);
        assert_eq!(Locale::parse("fr"), Locale::En);
    }

    #[test]
    fn saved_scene_mounts_3d_from_the_extract_dir() {
        let cli = Cli::parse_from([
            "rer_engine_editor",
            "--project-type",
            "3D",
            "--game-style",
            "first-person",
            "--save-path",
            "C:/projects/game.save",
            "--extract-dir",
            "C:/tmp/game",
        ]);

        let command = saved_scene_command(&cli).expect("debe montar la escena guardada");
        assert_eq!(command["cmd"], "set_scene");
        assert_eq!(command["scene"], "first-person");
        assert_eq!(command["save_path"], "C:/tmp/game");
    }

    #[test]
    fn saved_scene_mounts_2d_from_the_extract_dir() {
        let cli = Cli::parse_from([
            "rer_engine_editor",
            "--project-type",
            "2D",
            "--save-path",
            "C:/projects/game.save",
            "--extract-dir",
            "C:/tmp/game",
        ]);

        let command = saved_scene_command(&cli).expect("debe montar la escena guardada");
        assert_eq!(command["cmd"], "set_scene");
        assert_eq!(command["scene"], "2D");
    }

    #[test]
    fn no_saved_scene_without_a_save_file() {
        let cli = Cli::parse_from(["rer_engine_editor", "--project-type", "3D"]);
        assert!(saved_scene_command(&cli).is_none());
    }
}
