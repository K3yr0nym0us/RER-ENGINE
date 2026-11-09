//! Parseo de los argumentos con los que Electron lanza el editor.

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

/// Geometría física de la ventana del motor (coordenadas de pantalla + offsets
/// relativos al contenido de Electron). La usa el editor para lanzar el motor
/// en modo overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub rel_x: i32,
    pub rel_y: i32,
}

/// Configuración de arranque del editor egui.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cli {
    pub context: ProjectContext,
    pub save_path: Option<String>,
    pub extract_dir: Option<String>,
    pub locale: Locale,
    pub engine_parent_hwnd: u64,
    pub engine_bounds: Option<EngineBounds>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            context: ProjectContext::new(ProjectType::ThreeD, None),
            save_path: None,
            extract_dir: None,
            locale: Locale::En,
            engine_parent_hwnd: 0,
            engine_bounds: None,
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
        let mut engine_parent_hwnd: u64 = 0;
        let mut engine_x: i32 = 0;
        let mut engine_y: i32 = 0;
        let mut engine_width: Option<u32> = None;
        let mut engine_height: Option<u32> = None;
        let mut engine_rel_x: i32 = 0;
        let mut engine_rel_y: i32 = 0;

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
                "--parent-hwnd" => {
                    if let Some(value) = value {
                        engine_parent_hwnd = value.parse().unwrap_or(0);
                        index += 1;
                    }
                }
                "--engine-x" => {
                    if let Some(value) = value {
                        engine_x = value.parse().unwrap_or(0);
                        index += 1;
                    }
                }
                "--engine-y" => {
                    if let Some(value) = value {
                        engine_y = value.parse().unwrap_or(0);
                        index += 1;
                    }
                }
                "--engine-width" => {
                    if let Some(value) = value {
                        engine_width = value.parse().ok();
                        index += 1;
                    }
                }
                "--engine-height" => {
                    if let Some(value) = value {
                        engine_height = value.parse().ok();
                        index += 1;
                    }
                }
                "--engine-rel-x" => {
                    if let Some(value) = value {
                        engine_rel_x = value.parse().unwrap_or(0);
                        index += 1;
                    }
                }
                "--engine-rel-y" => {
                    if let Some(value) = value {
                        engine_rel_y = value.parse().unwrap_or(0);
                        index += 1;
                    }
                }
                _ => {}
            }

            index += 1;
        }

        Self {
            context: ProjectContext::new(project_type, game_style.as_deref()),
            save_path,
            extract_dir,
            locale,
            engine_parent_hwnd,
            engine_bounds: match (engine_width, engine_height) {
                (Some(width), Some(height)) if width > 0 && height > 0 => Some(EngineBounds {
                    x: engine_x,
                    y: engine_y,
                    width,
                    height,
                    rel_x: engine_rel_x,
                    rel_y: engine_rel_y,
                }),
                _ => None,
            },
        }
    }

    pub fn usage() -> &'static str {
        "rer_engine_editor --project-type <2D|3D> [--game-style <estilo>] \
[--save-path <ruta>] [--extract-dir <ruta>] [--locale <en|es>] \
[--parent-hwnd <u64> --engine-x <i32> --engine-y <i32> \
--engine-width <u32> --engine-height <u32> --engine-rel-x <i32> --engine-rel-y <i32>]"
    }
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
    fn parses_engine_geometry() {
        let cli = Cli::parse_from([
            "rer_engine_editor",
            "--parent-hwnd",
            "123456",
            "--engine-x",
            "-20",
            "--engine-y",
            "30",
            "--engine-width",
            "800",
            "--engine-height",
            "600",
            "--engine-rel-x",
            "12",
            "--engine-rel-y",
            "34",
        ]);

        assert_eq!(cli.engine_parent_hwnd, 123456);
        assert_eq!(
            cli.engine_bounds,
            Some(EngineBounds {
                x: -20,
                y: 30,
                width: 800,
                height: 600,
                rel_x: 12,
                rel_y: 34,
            })
        );
    }
}
