//! Tipo de proyecto y estilo de cámara que condicionan la UI del editor.
//!
//! Estas reglas son el espejo de las validaciones de `SidebarLeft` /
//! `SidebarRight` en React, para que el marco egui muestre solo lo que
//! corresponde a un proyecto 2D o 3D.

/// Tipo de proyecto del motor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectType {
    TwoD,
    ThreeD,
}

impl ProjectType {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "2D" => Some(Self::TwoD),
            "3D" => Some(Self::ThreeD),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::TwoD => "2D",
            Self::ThreeD => "3D",
        }
    }

    /// Estilo de cámara por defecto cuando no se recibe uno válido.
    pub fn default_game_style(self) -> GameStyle {
        match self {
            Self::TwoD => GameStyle::TopDown,
            Self::ThreeD => GameStyle::FirstPerson,
        }
    }
}

/// Estilo de cámara / perspectiva (mismos valores que `GameStyle` en TypeScript).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameStyle {
    FirstPerson,
    SecondPerson,
    ThirdPerson,
    TopDown,
    SideScroller,
    Isometric,
}

impl GameStyle {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "first-person" => Some(Self::FirstPerson),
            "second-person" => Some(Self::SecondPerson),
            "third-person" => Some(Self::ThirdPerson),
            "top-down" => Some(Self::TopDown),
            "side-scroller" => Some(Self::SideScroller),
            "isometric" => Some(Self::Isometric),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::FirstPerson => "first-person",
            Self::SecondPerson => "second-person",
            Self::ThirdPerson => "third-person",
            Self::TopDown => "top-down",
            Self::SideScroller => "side-scroller",
            Self::Isometric => "isometric",
        }
    }

    /// Estilos que solo tienen sentido en un proyecto 3D.
    pub fn is_3d_camera(self) -> bool {
        matches!(
            self,
            Self::FirstPerson | Self::SecondPerson | Self::ThirdPerson
        )
    }

    /// ¿El estilo pertenece al tipo de proyecto indicado?
    pub fn matches_project_type(self, project_type: ProjectType) -> bool {
        match project_type {
            ProjectType::TwoD => !self.is_3d_camera(),
            ProjectType::ThreeD => self.is_3d_camera(),
        }
    }
}

/// Contexto validado que consume el marco del editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectContext {
    pub project_type: ProjectType,
    pub game_style: GameStyle,
    /// `true` cuando el estilo recibido no era válido para el tipo de proyecto
    /// y se sustituyó por el valor por defecto.
    pub game_style_adjusted: bool,
}

impl ProjectContext {
    /// Normaliza `game_style` según el tipo de proyecto.
    pub fn new(project_type: ProjectType, game_style: Option<&str>) -> Self {
        let default_style = project_type.default_game_style();
        let parsed = game_style.and_then(GameStyle::parse);

        let (game_style, game_style_adjusted) = match parsed {
            Some(style) if style.matches_project_type(project_type) => (style, false),
            Some(_) => (default_style, true),
            None => (default_style, game_style.is_some()),
        };

        Self {
            project_type,
            game_style,
            game_style_adjusted,
        }
    }

    #[allow(dead_code)] // La Fase 1 (marco dividido) consume estas reglas.
    pub fn is_3d(self) -> bool {
        self.project_type == ProjectType::ThreeD
    }

    /// En React "Camera" y "Resources" existen para 2D y 3D (no para otros tipos).
    #[allow(dead_code)] // La Fase 3 (sidebar izquierda) consume esta regla.
    pub fn shows_camera_and_resources(self) -> bool {
        matches!(self.project_type, ProjectType::TwoD | ProjectType::ThreeD)
    }

    /// `SidebarRight`: "Background" solo aparece en 2D.
    #[allow(dead_code)] // La Fase 4 (sidebar derecha) consume esta regla.
    pub fn shows_world_background(self) -> bool {
        !self.is_3d()
    }

    /// `SidebarRight`: iluminación, texturas, reflejos, sombras, MSAA y TAA solo en 3D.
    #[allow(dead_code)] // La Fase 4 (sidebar derecha) consume esta regla.
    pub fn shows_world_3d_sections(self) -> bool {
        self.is_3d()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_project_types_case_insensitively() {
        assert_eq!(ProjectType::parse("2d"), Some(ProjectType::TwoD));
        assert_eq!(ProjectType::parse(" 3D "), Some(ProjectType::ThreeD));
        assert_eq!(ProjectType::parse("4D"), None);
    }

    #[test]
    fn parses_all_game_styles() {
        for (raw, expected) in [
            ("first-person", GameStyle::FirstPerson),
            ("second-person", GameStyle::SecondPerson),
            ("third-person", GameStyle::ThirdPerson),
            ("top-down", GameStyle::TopDown),
            ("side-scroller", GameStyle::SideScroller),
            ("isometric", GameStyle::Isometric),
        ] {
            assert_eq!(GameStyle::parse(raw), Some(expected));
            assert_eq!(expected.as_str(), raw);
        }
    }

    #[test]
    fn defaults_style_per_project_type() {
        let two_d = ProjectContext::new(ProjectType::TwoD, None);
        assert_eq!(two_d.game_style, GameStyle::TopDown);
        assert!(!two_d.game_style_adjusted);

        let three_d = ProjectContext::new(ProjectType::ThreeD, None);
        assert_eq!(three_d.game_style, GameStyle::FirstPerson);
        assert!(!three_d.game_style_adjusted);
    }

    #[test]
    fn rejects_3d_style_for_2d_project() {
        let context = ProjectContext::new(ProjectType::TwoD, Some("third-person"));
        assert_eq!(context.project_type, ProjectType::TwoD);
        assert_eq!(context.game_style, GameStyle::TopDown);
        assert!(context.game_style_adjusted);
    }

    #[test]
    fn rejects_2d_style_for_3d_project() {
        let context = ProjectContext::new(ProjectType::ThreeD, Some("side-scroller"));
        assert_eq!(context.project_type, ProjectType::ThreeD);
        assert_eq!(context.game_style, GameStyle::FirstPerson);
        assert!(context.game_style_adjusted);
    }

    #[test]
    fn flags_unknown_style_as_adjusted() {
        let context = ProjectContext::new(ProjectType::ThreeD, Some("isometric-ish"));
        assert_eq!(context.game_style, GameStyle::FirstPerson);
        assert!(context.game_style_adjusted);
    }

    #[test]
    fn keeps_valid_style() {
        let context = ProjectContext::new(ProjectType::ThreeD, Some("third-person"));
        assert_eq!(context.game_style, GameStyle::ThirdPerson);
        assert!(!context.game_style_adjusted);
    }

    #[test]
    fn world_sections_follow_project_type() {
        let two_d = ProjectContext::new(ProjectType::TwoD, None);
        assert!(two_d.shows_camera_and_resources());
        assert!(two_d.shows_world_background());
        assert!(!two_d.shows_world_3d_sections());

        let three_d = ProjectContext::new(ProjectType::ThreeD, None);
        assert!(three_d.shows_camera_and_resources());
        assert!(!three_d.shows_world_background());
        assert!(three_d.shows_world_3d_sections());
    }
}
