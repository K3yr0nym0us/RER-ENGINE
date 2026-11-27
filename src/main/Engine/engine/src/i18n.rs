use std::collections::HashMap;
use std::path::Path;

use rer_engine_core::i18n::{
    Catalog, CatalogEntry, CatalogSet, FormatError, I18nConfig, LocaleId, MessageTemplate,
    ResolvedText, TextValue,
};

use crate::text::{FontFamily, TextSystem};

/// Host-readable copy of the runtime i18n state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct I18nSnapshot {
    pub catalogs: CatalogSet,
    pub active_language: LocaleId,
    pub formatting_locale: LocaleId,
    pub epoch: u64,
}

/// Runtime ownership for catalogs, locale selection and text shaping.
///
/// Catalog paths in [`I18nConfig`] are resolved against the extracted project
/// directory. The stored configuration keeps those resolved paths so subsequent
/// reloads are unambiguous.
#[derive(Debug)]
pub struct I18nRuntime {
    catalogs: CatalogSet,
    text_system: TextSystem,
    /// Ruta canónica → alias de familia registrado en `text_system`.
    font_aliases: HashMap<String, String>,
    active_language: LocaleId,
    formatting_locale: LocaleId,
    epoch: u64,
    config: I18nConfig,
}

const BUILT_IN_GAME_MESSAGES: &[(&str, &str, &str)] = &[
    ("interaction.press", "Press", "Pulsa"),
    ("interaction.hold", "Hold", "Mantén"),
    ("interaction.to_interact", "To interact", "Para interactuar"),
];

fn built_in_catalogs() -> Vec<Catalog> {
    let mut english = Catalog::new(LocaleId::parse("en").expect("built-in locale"), "game");
    let mut spanish = Catalog::new(LocaleId::parse("es").expect("built-in locale"), "game");
    for &(key, source, translated) in BUILT_IN_GAME_MESSAGES {
        english.insert(
            key,
            CatalogEntry {
                source: source.to_owned(),
                template: MessageTemplate::text(source),
            },
        );
        spanish.insert(
            key,
            CatalogEntry {
                source: source.to_owned(),
                template: MessageTemplate::text(translated),
            },
        );
    }
    vec![english, spanish]
}

/// Adds built-ins at lower precedence than project catalogs.
///
/// `CatalogSet` resolves the first matching catalog. Removing project-owned
/// IDs from the built-ins before appending project catalogs preserves both the
/// declared precedence among project catalogs and per-message built-in fallback.
fn merge_built_in_catalogs(project_catalogs: Vec<Catalog>) -> Vec<Catalog> {
    let mut catalogs = built_in_catalogs();
    for project in &project_catalogs {
        if let Some(built_in) = catalogs.iter_mut().find(|catalog| {
            catalog.locale == project.locale && catalog.namespace == project.namespace
        }) {
            for id in project.entries.keys() {
                built_in.entries.remove(id);
            }
        }
    }
    catalogs.extend(project_catalogs);
    catalogs
}

impl Default for I18nRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl I18nRuntime {
    /// Creates an English runtime without catalog files.
    #[must_use]
    pub fn new() -> Self {
        let config = I18nConfig::default();
        let active_language = config.default_language.clone();
        let formatting_locale = active_language.clone();
        let catalogs = CatalogSet::new(
            config.default_language.clone(),
            config.fallback_language.clone(),
            built_in_catalogs(),
        );
        let mut text_system = TextSystem::new();
        text_system.set_locale(active_language.to_string());

        Self {
            catalogs,
            text_system,
            font_aliases: HashMap::new(),
            active_language,
            formatting_locale,
            epoch: 0,
            config,
        }
    }

    /// Loads project catalogs and applies its default language.
    ///
    /// Returns whether the effective catalogs or selected locales changed. A
    /// missing configuration uses the built-in English defaults. Every successful
    /// load recreates `TextSystem` so custom fonts cannot leak between projects.
    pub fn load(
        &mut self,
        extract_dir: &Path,
        config: Option<&I18nConfig>,
    ) -> Result<bool, String> {
        let config = resolve_config(extract_dir, config);
        let mut catalogs = config.load_catalogs()?;
        catalogs.catalogs = merge_built_in_catalogs(std::mem::take(&mut catalogs.catalogs));
        let active_language = config.default_language.clone();
        let formatting_locale = config.default_language.clone();
        let changed = self.catalogs != catalogs
            || self.active_language != active_language
            || self.formatting_locale != formatting_locale;

        self.catalogs = catalogs;
        self.config = config;
        self.active_language = active_language.clone();
        self.formatting_locale = formatting_locale;
        // Un proyecto nuevo no puede heredar familias custom ni IDs de fontdb del
        // anterior. El registro se repobla bajo demanda con `ensure_font`.
        self.text_system = TextSystem::new();
        self.font_aliases.clear();
        self.text_system.set_locale(active_language.to_string());
        if changed {
            self.epoch = self.epoch.wrapping_add(1);
        }
        Ok(changed)
    }

    /// Selects the language used for catalog lookup and text shaping.
    pub fn set_language(&mut self, language: LocaleId) -> bool {
        if self.active_language == language {
            return false;
        }

        self.active_language = language.clone();
        self.text_system.set_locale(language.to_string());
        self.epoch = self.epoch.wrapping_add(1);
        true
    }

    /// Selects the independent formatting locale exposed to scripts.
    pub fn set_locale(&mut self, locale: LocaleId) -> bool {
        if self.formatting_locale == locale {
            return false;
        }

        self.formatting_locale = locale;
        self.epoch = self.epoch.wrapping_add(1);
        true
    }

    /// Resolves localized or literal text in the active language.
    pub fn resolve(&self, value: &TextValue) -> Result<ResolvedText, FormatError> {
        self.catalogs.resolve_value(&self.active_language, value)
    }

    /// Language used to shape a HUD text value.
    ///
    /// Localized text follows the active catalog language. Literal text uses
    /// `und` so script fallback is selected from its contents instead of the
    /// project's active language.
    #[must_use]
    pub fn text_shaping_language(&self, value: &TextValue) -> String {
        if value.is_localized() {
            self.active_language.to_string()
        } else {
            "und".to_owned()
        }
    }

    #[must_use]
    pub const fn active_language(&self) -> &LocaleId {
        &self.active_language
    }

    #[must_use]
    pub const fn formatting_locale(&self) -> &LocaleId {
        &self.formatting_locale
    }

    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    #[must_use]
    pub const fn catalogs(&self) -> &CatalogSet {
        &self.catalogs
    }

    #[must_use]
    pub const fn config(&self) -> &I18nConfig {
        &self.config
    }

    #[must_use]
    pub const fn text_system(&self) -> &TextSystem {
        &self.text_system
    }

    #[must_use]
    pub fn text_system_mut(&mut self) -> &mut TextSystem {
        &mut self.text_system
    }

    /// Registra una fuente de proyecto una sola vez y devuelve su familia custom.
    ///
    /// Una ruta vacía usa la familia sans-serif embebida. Si el alias llega vacío,
    /// se usa el `file_stem` de la ruta; si ese nombre es reservado se prefija con
    /// `Project `. Si la ruta ya estaba registrada se conserva el primer alias para
    /// que los glifos y las claves de caché sean estables.
    ///
    /// # Errors
    /// Devuelve un error si fontdb no puede leer la fuente o un alias explícito
    /// coincide con una familia reservada.
    pub fn ensure_font(&mut self, path: &str, alias: &str) -> Result<FontFamily, String> {
        if path.trim().is_empty() {
            return Ok(FontFamily::SansSerif);
        }
        let key = font_path_key(path);
        if let Some(registered) = self.font_aliases.get(&key) {
            return Ok(FontFamily::Custom(registered.clone()));
        }

        let alias = alias.trim();
        let alias = if alias.is_empty() {
            let stem = Path::new(path)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::trim)
                .filter(|stem| !stem.is_empty())
                .unwrap_or("Project Font");
            if FontFamily::is_reserved_name(stem) {
                format!("Project {stem}")
            } else {
                stem.to_owned()
            }
        } else {
            alias.to_owned()
        };
        self.text_system.load_font(alias.clone(), path)?;
        self.font_aliases.insert(key, alias.clone());
        Ok(FontFamily::Custom(alias))
    }

    /// Elimina una fuente custom y reconstruye el sistema con las restantes.
    #[must_use]
    pub fn remove_font(&mut self, path: &str) -> bool {
        let key = font_path_key(path);
        if self.font_aliases.remove(&key).is_none() {
            return false;
        }

        self.reset_text_system();
        let registered = self.font_aliases.clone();
        for (path, alias) in registered {
            if let Err(error) = self.text_system.load_font(alias.clone(), &path) {
                self.font_aliases.remove(&path);
                log::warn!("[font] no se pudo restaurar {path} tras eliminar otra fuente: {error}");
            }
        }
        true
    }

    fn reset_text_system(&mut self) {
        let locale = self.active_language.to_string();
        self.text_system = TextSystem::new();
        self.text_system.set_locale(locale);
    }

    /// Takes a host-readable snapshot without borrowing runtime internals.
    #[must_use]
    pub fn snapshot(&self) -> I18nSnapshot {
        I18nSnapshot {
            catalogs: self.catalogs.clone(),
            active_language: self.active_language.clone(),
            formatting_locale: self.formatting_locale.clone(),
            epoch: self.epoch,
        }
    }
}

fn font_path_key(path: &str) -> String {
    std::fs::canonicalize(path)
        .map(|canonical| canonical.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_owned())
}

/// Valida el contrato raíz compartido por los proyectos 2D y 3D.
///
/// El runtime no conserva compatibilidad con manifiestos versionados: `i18n` es
/// obligatorio y los antiguos `version`/`language` se rechazan antes de cargar
/// cualquier escena o recurso.
pub(crate) fn validate_project_manifest(manifest: &serde_json::Value) -> Result<(), String> {
    if manifest.get("version").is_some() {
        return Err("manifest.json inválido: el campo version legacy no está permitido".to_owned());
    }
    if manifest.get("language").is_some() {
        return Err(
            "manifest.json inválido: el campo language legacy no está permitido".to_owned(),
        );
    }
    let i18n = manifest
        .get("i18n")
        .ok_or_else(|| "manifest.json inválido: falta la sección i18n".to_owned())?;
    serde_json::from_value::<I18nConfig>(i18n.clone())
        .map(|_| ())
        .map_err(|error| format!("manifest.json inválido (i18n): {error}"))
}

fn resolve_config(extract_dir: &Path, config: Option<&I18nConfig>) -> I18nConfig {
    let mut config = config.cloned().unwrap_or_default();
    config.catalog_paths = config
        .catalog_paths
        .into_iter()
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                extract_dir.join(path)
            }
        })
        .collect();
    config
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use rer_engine_core::i18n::{Catalog, CatalogEntry, LocalizedText, MessageTemplate, TextId};

    use super::*;

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "rer-i18n-runtime-{tag}-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create runtime test directory");
        dir
    }

    fn locale(value: &str) -> LocaleId {
        LocaleId::parse(value).expect("valid test locale")
    }

    fn write_catalog(
        dir: &Path,
        relative_path: &str,
        locale_name: &str,
        entries: &[(&str, &str, &str)],
    ) {
        let mut catalog = Catalog::new(locale(locale_name), "game");
        for (key, source, translated) in entries {
            catalog.insert(
                *key,
                CatalogEntry {
                    source: (*source).to_owned(),
                    template: MessageTemplate::text(*translated),
                },
            );
        }
        let path = dir.join(relative_path);
        std::fs::create_dir_all(path.parent().expect("catalog parent"))
            .expect("create catalog directory");
        std::fs::write(path, catalog.to_json_pretty().expect("serialize catalog"))
            .expect("write catalog");
    }

    fn localized(key: &str, source: &str) -> TextValue {
        TextValue::Localized(LocalizedText::new(TextId::new("game", key), source))
    }

    #[test]
    fn defaults_to_english_without_configuration() {
        let dir = temp_dir("defaults");
        let mut runtime = I18nRuntime::new();

        assert!(!runtime.load(&dir, None).expect("load built-in defaults"));
        assert_eq!(runtime.active_language(), &locale("en"));
        assert_eq!(runtime.formatting_locale(), &locale("en"));
        assert_eq!(runtime.epoch(), 0);
        assert_eq!(runtime.text_system().locale(), "en");
        assert_eq!(runtime.catalogs().catalogs, built_in_catalogs());
        assert!(runtime.config().catalog_paths.is_empty());

        std::fs::remove_dir_all(dir).expect("remove runtime test directory");
    }

    #[test]
    fn text_shaping_language_is_active_only_for_localized_values() {
        let mut runtime = I18nRuntime::new();
        assert!(runtime.set_language(locale("ar")));

        assert_eq!(
            runtime.text_shaping_language(&TextValue::literal("مرحبا")),
            "und"
        );
        assert_eq!(
            runtime.text_shaping_language(&localized("title", "Title")),
            "ar"
        );
    }

    #[test]
    fn ensure_font_registers_once_and_project_load_clears_custom_fonts() {
        let dir = temp_dir("ensure-font");
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/fonts/Inter_18pt-Regular.ttf");
        let path = path.to_string_lossy().into_owned();
        let mut runtime = I18nRuntime::new();

        assert_eq!(
            runtime.ensure_font("", "").expect("built-in fallback"),
            crate::text::FontFamily::SansSerif
        );
        let first = runtime
            .ensure_font(&path, "Project Inter")
            .expect("register project font");
        assert_eq!(
            first,
            crate::text::FontFamily::Custom("Project Inter".to_owned())
        );
        assert_eq!(
            runtime
                .ensure_font(&path, "Ignored Alias")
                .expect("reuse registered path"),
            first
        );

        runtime.load(&dir, None).expect("load new project");
        let reloaded = runtime
            .ensure_font(&path, "Reloaded Inter")
            .expect("register font after project reset");
        assert_eq!(
            reloaded,
            crate::text::FontFamily::Custom("Reloaded Inter".to_owned())
        );
        assert_ne!(reloaded, first);
        assert!(runtime.remove_font(&path));
        assert!(!runtime.remove_font(&path));

        std::fs::remove_dir_all(dir).expect("remove runtime test directory");
    }

    #[test]
    fn empty_font_alias_uses_file_stem_and_prefixes_reserved_names() {
        let dir = temp_dir("ensure-font-default-alias");
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/fonts/Inter_18pt-Regular.ttf");
        let regular_path = dir.join("Inter_18pt-Regular.ttf");
        std::fs::copy(&source, &regular_path).expect("copy project font");
        let reserved_path = dir.join("monospace.ttf");
        std::fs::copy(&source, &reserved_path).expect("copy reserved-name font");
        let mut runtime = I18nRuntime::new();

        assert_eq!(
            runtime
                .ensure_font(&regular_path.to_string_lossy(), "  ")
                .expect("derive alias from file stem"),
            crate::text::FontFamily::Custom("Inter_18pt-Regular".to_owned())
        );
        assert_eq!(
            runtime
                .ensure_font(&reserved_path.to_string_lossy(), "")
                .expect("prefix reserved file stem"),
            crate::text::FontFamily::Custom("Project monospace".to_owned())
        );

        std::fs::remove_dir_all(dir).expect("remove runtime test directory");
    }

    #[test]
    fn project_catalogs_override_built_in_game_messages() {
        let dir = temp_dir("builtin_override");
        write_catalog(
            &dir,
            "es.json",
            "es",
            &[("interaction.press", "Press", "Pulsa proyecto")],
        );
        let config = I18nConfig {
            default_language: locale("es"),
            fallback_language: locale("en"),
            catalog_paths: vec![PathBuf::from("es.json")],
        };
        let mut runtime = I18nRuntime::new();

        assert!(runtime.load(&dir, Some(&config)).expect("load catalogs"));
        assert_eq!(
            runtime
                .resolve(&localized("interaction.press", "Press"))
                .expect("resolve project override")
                .display,
            "Pulsa proyecto"
        );
        assert_eq!(
            runtime
                .resolve(&localized("interaction.to_interact", "To interact"))
                .expect("resolve built-in fallback")
                .display,
            "Para interactuar"
        );

        std::fs::remove_dir_all(dir).expect("remove runtime test directory");
    }

    #[test]
    fn loads_relative_catalogs_and_resolves_localized_and_literal_values() {
        let dir = temp_dir("load");
        write_catalog(
            &dir,
            "catalogs/en.json",
            "en",
            &[("hello", "Hello", "Hello")],
        );
        write_catalog(
            &dir,
            "catalogs/es.json",
            "es",
            &[("hello", "Hello", "Hola")],
        );
        let config = I18nConfig {
            default_language: locale("en"),
            fallback_language: locale("en"),
            catalog_paths: vec![
                PathBuf::from("catalogs/en.json"),
                PathBuf::from("catalogs/es.json"),
            ],
        };
        let mut runtime = I18nRuntime::new();

        assert!(runtime.load(&dir, Some(&config)).expect("load catalogs"));
        assert_eq!(runtime.epoch(), 1);
        assert_eq!(
            runtime.config().catalog_paths[0],
            dir.join("catalogs/en.json")
        );
        assert!(runtime.set_language(locale("es")));

        let translated = runtime
            .resolve(&localized("hello", "Hello"))
            .expect("resolve");
        assert_eq!(translated.display, "Hola");
        assert_eq!(translated.locale, locale("es"));
        assert!(!translated.stale);
        assert_eq!(runtime.text_system().locale(), "es");

        let literal = TextValue::literal("Literal text");
        let resolved = runtime.resolve(&literal).expect("resolve literal");
        assert_eq!(resolved.display, "Literal text");
        assert_eq!(resolved.locale, locale("es"));
        assert!(!resolved.stale);

        std::fs::remove_dir_all(dir).expect("remove runtime test directory");
    }

    #[test]
    fn falls_back_per_message_then_to_the_source_literal() {
        let dir = temp_dir("fallback");
        write_catalog(
            &dir,
            "en.json",
            "en",
            &[("fallback", "Fallback", "English fallback")],
        );
        write_catalog(&dir, "es.json", "es", &[("hello", "Hello", "Hola")]);
        let config = I18nConfig {
            default_language: locale("es"),
            fallback_language: locale("en"),
            catalog_paths: vec![PathBuf::from("en.json"), PathBuf::from("es.json")],
        };
        let mut runtime = I18nRuntime::new();
        runtime.load(&dir, Some(&config)).expect("load catalogs");

        let translated = runtime
            .resolve(&localized("hello", "Hello"))
            .expect("resolve active language");
        assert_eq!(translated.display, "Hola");
        assert_eq!(translated.locale, locale("es"));

        let fallback = runtime
            .resolve(&localized("fallback", "Fallback"))
            .expect("resolve fallback");
        assert_eq!(fallback.display, "English fallback");
        assert_eq!(fallback.locale, locale("en"));

        let missing = runtime
            .resolve(&localized("missing", "Source text"))
            .expect("resolve missing");
        assert_eq!(missing.display, "Source text");
        assert_eq!(missing.locale, locale("en"));
        assert!(missing.stale);

        std::fs::remove_dir_all(dir).expect("remove runtime test directory");
    }

    #[test]
    fn language_and_formatting_changes_are_noops_or_advance_the_epoch() {
        let mut runtime = I18nRuntime::new();
        let initial = runtime.snapshot();

        assert!(runtime.set_language(locale("es")));
        assert_eq!(runtime.epoch(), initial.epoch.wrapping_add(1));
        assert_eq!(runtime.text_system().locale(), "es");
        assert!(!runtime.set_language(locale("es")));
        assert_eq!(runtime.epoch(), initial.epoch.wrapping_add(1));

        assert!(runtime.set_locale(locale("es-MX")));
        assert_eq!(runtime.epoch(), initial.epoch.wrapping_add(2));
        assert_eq!(runtime.formatting_locale(), &locale("es-MX"));
        assert_eq!(runtime.active_language(), &locale("es"));
        assert!(!runtime.set_locale(locale("es-mx")));
        assert_eq!(runtime.epoch(), initial.epoch.wrapping_add(2));

        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.active_language, locale("es"));
        assert_eq!(snapshot.formatting_locale, locale("es-MX"));
        assert_eq!(snapshot.epoch, initial.epoch.wrapping_add(2));
    }

    #[test]
    fn reloading_catalogs_with_the_same_language_is_a_change() {
        let dir = temp_dir("catalog-reload");
        let config = I18nConfig {
            default_language: locale("en"),
            fallback_language: locale("en"),
            catalog_paths: vec![PathBuf::from("game.json")],
        };
        let mut runtime = I18nRuntime::new();

        write_catalog(
            &dir,
            "game.json",
            "en",
            &[("hello", "Hello", "First translation")],
        );
        assert!(
            runtime
                .load(&dir, Some(&config))
                .expect("load first catalog")
        );
        assert_eq!(runtime.epoch(), 1);

        write_catalog(
            &dir,
            "game.json",
            "en",
            &[("hello", "Hello", "Second translation")],
        );
        assert!(
            runtime
                .load(&dir, Some(&config))
                .expect("reload changed catalog")
        );
        assert_eq!(runtime.epoch(), 2);
        assert_eq!(runtime.active_language(), &locale("en"));
        assert_eq!(runtime.formatting_locale(), &locale("en"));
        assert_eq!(
            runtime
                .resolve(&localized("hello", "Hello"))
                .expect("resolve reloaded catalog")
                .display,
            "Second translation"
        );

        assert!(
            !runtime
                .load(&dir, Some(&config))
                .expect("reload unchanged catalog")
        );
        assert_eq!(runtime.epoch(), 2);

        std::fs::remove_dir_all(dir).expect("remove runtime test directory");
    }

    #[test]
    fn reloading_the_same_effective_state_does_not_advance_the_epoch() {
        let dir = temp_dir("reload");
        let config = I18nConfig {
            default_language: locale("en"),
            fallback_language: locale("en"),
            catalog_paths: Vec::new(),
        };
        let mut runtime = I18nRuntime::new();

        assert!(!runtime.load(&dir, Some(&config)).expect("load defaults"));
        assert!(!runtime.load(&dir, Some(&config)).expect("reload defaults"));
        assert_eq!(runtime.epoch(), 0);
        assert!(runtime.set_language(locale("es")));
        assert!(
            runtime
                .load(&dir, Some(&config))
                .expect("restore project defaults")
        );
        assert_eq!(runtime.active_language(), &locale("en"));
        assert_eq!(runtime.epoch(), 2);

        std::fs::remove_dir_all(dir).expect("remove runtime test directory");
    }
}
