use cosmic_text::{Fallback, FontSystem};
use fontdb::{Database, ID, Source};
use unicode_script::Script;

use crate::embedded_fonts;

/// Una de las seis fuentes incluidas en el binario del motor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontSource {
    InterRegular,
    InterBold,
    NotoArabic,
    NotoHebrew,
    NotoSansSC,
    FiraMono,
}

impl FontSource {
    pub const ALL: [Self; 6] = [
        Self::InterRegular,
        Self::InterBold,
        Self::NotoArabic,
        Self::NotoHebrew,
        Self::NotoSansSC,
        Self::FiraMono,
    ];

    /// Nombre de familia canónico usado por fontdb y cosmic-text.
    pub const fn family(self) -> &'static str {
        match self {
            Self::InterRegular | Self::InterBold => "Inter 18pt",
            Self::NotoArabic => "Noto Sans Arabic",
            Self::NotoHebrew => "Noto Sans Hebrew",
            Self::NotoSansSC => "Noto Sans SC",
            Self::FiraMono => "Fira Mono",
        }
    }

    /// Bytes TTF empaquetados con `include_bytes!`.
    pub const fn bytes(self) -> &'static [u8] {
        match self {
            Self::InterRegular => embedded_fonts::INTER_REGULAR,
            Self::InterBold => embedded_fonts::INTER_BOLD,
            Self::NotoArabic => embedded_fonts::NOTO_SANS_ARABIC,
            Self::NotoHebrew => embedded_fonts::NOTO_SANS_HEBREW,
            Self::NotoSansSC => embedded_fonts::NOTO_SANS_SC,
            Self::FiraMono => embedded_fonts::FIRA_MONO,
        }
    }

    fn load(self, db: &mut Database) -> ID {
        let ids = db.load_font_source(Source::Binary(std::sync::Arc::new(self.bytes())));
        ids.first()
            .copied()
            .unwrap_or_else(|| panic!("failed to load embedded font {}", self.family()))
    }
}

/// Metadatos de una cara cargada en la base de fontdb.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedFont {
    pub source: FontSource,
    pub id: ID,
    pub family: &'static str,
    pub families: Vec<String>,
    pub weight: fontdb::Weight,
    pub monospaced: bool,
}

/// Fallback determinista: solo consulta familias presentes en el binario.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EmbeddedFallback;

impl Fallback for EmbeddedFallback {
    fn common_fallback(&self) -> &[&'static str] {
        &[
            "Inter 18pt",
            "Noto Sans SC",
            "Noto Sans Hebrew",
            "Noto Sans Arabic",
            "Fira Mono",
        ]
    }

    fn forbidden_fallback(&self) -> &[&'static str] {
        &[]
    }

    fn script_fallback(&self, script: Script, _locale: &str) -> &[&'static str] {
        match script {
            Script::Arabic => &["Noto Sans Arabic", "Inter 18pt"],
            Script::Hebrew => &["Noto Sans Hebrew", "Inter 18pt"],
            Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul => {
                &["Noto Sans SC", "Inter 18pt"]
            }
            _ => &["Inter 18pt", "Fira Mono"],
        }
    }
}

pub(crate) fn load(db: &mut Database) -> Vec<LoadedFont> {
    FontSource::ALL
        .into_iter()
        .map(|source| {
            let id = source.load(db);
            let face = db
                .face(id)
                .unwrap_or_else(|| panic!("missing loaded font {}", source.family()));
            LoadedFont {
                source,
                id,
                family: source.family(),
                families: face
                    .families
                    .iter()
                    .map(|(family, _)| family.clone())
                    .collect(),
                weight: face.weight,
                monospaced: face.monospaced,
            }
        })
        .collect()
}

pub(crate) fn load_custom(db: &mut Database, family: &str, bytes: Vec<u8>) -> Result<ID, String> {
    // Parseamos en una DB temporal para poder añadir el alias del proyecto a
    // FaceInfo antes de insertarlo. fontdb no expone una API de alias directa.
    let mut parsed = Database::new();
    let ids = parsed.load_font_source(Source::Binary(std::sync::Arc::new(bytes)));
    if ids.is_empty() {
        return Err("font bytes do not contain a valid font face".to_owned());
    }

    let mut faces = Vec::new();
    for id in ids {
        if let Some(face) = parsed.face(id) {
            faces.push(face.clone());
        }
    }
    if faces.is_empty() {
        return Err("font bytes do not contain a usable font face".to_owned());
    }

    // cosmic-text solo acepta una familia nombrada como primaria si el peso
    // coincide exactamente. Registramos los dos pesos de nuestra API usando la
    // cara real más cercana; en una fuente variable HarfBuzz aplicará además el
    // eje `wght`, y en una estática se conserva su outline.
    let mut first = None;
    for target_weight in [fontdb::Weight::NORMAL, fontdb::Weight::BOLD] {
        let Some(mut face) = faces
            .iter()
            .min_by_key(|face| face.weight.0.abs_diff(target_weight.0))
            .cloned()
        else {
            continue;
        };
        let language = face
            .families
            .first()
            .map(|(_, language)| *language)
            .unwrap_or(fontdb::Language::English_UnitedStates);
        face.families.insert(0, (family.to_owned(), language));
        face.weight = target_weight;
        face.id = ID::dummy();
        let id = db.push_face_info(face);
        if target_weight == fontdb::Weight::NORMAL {
            first = Some(id);
        }
    }
    first.ok_or_else(|| "font bytes do not contain a usable font face".to_owned())
}

pub(crate) fn font_system(locale: String, mut db: Database) -> FontSystem {
    db.set_sans_serif_family(FontSource::InterRegular.family());
    db.set_serif_family(FontSource::InterRegular.family());
    db.set_cursive_family(FontSource::InterRegular.family());
    db.set_fantasy_family(FontSource::InterRegular.family());
    db.set_monospace_family(FontSource::FiraMono.family());
    FontSystem::new_with_locale_and_db_and_fallback(locale, db, EmbeddedFallback)
}
