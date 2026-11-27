use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::de::Error as _;
use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::i18n::types::CatalogEntry;
use crate::i18n::{FormatError, LocaleId, MessageTemplate, PluralCategory, TextId, format};

/// One locale's messages for a single namespace.
///
/// Entries remain indexed by [`TextId`] for efficient exact lookup. Their JSON
/// representation is an explicit array of editable records rather than a map
/// with implementation-defined keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    pub locale: LocaleId,
    pub namespace: String,
    pub entries: BTreeMap<TextId, CatalogEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogWire {
    locale: LocaleId,
    namespace: String,
    entries: Vec<CatalogEntryWire>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogEntryWire {
    id: TextId,
    source: String,
    template: MessageTemplate,
}

fn validate_entry(source: &str, template: &MessageTemplate) -> Result<(), String> {
    let source_placeholders = placeholder_names(source)
        .map_err(|error| format!("invalid source placeholders: {error}"))?;

    match (&template.text, &template.plural_forms) {
        (Some(_), Some(_)) => Err("template must not define both text and plural forms".to_owned()),
        (None, None) => Err("template must define either text or plural forms".to_owned()),
        (Some(text), None) => validate_template_text(text, &source_placeholders, "text"),
        (None, Some(forms)) => {
            if forms.is_empty() {
                return Err("plural forms must not be empty".to_owned());
            }
            if !forms.contains_key(&PluralCategory::Other) {
                return Err("plural forms must include the 'other' category".to_owned());
            }
            for (category, text) in forms {
                let location = format!("{category} plural form");
                validate_template_text(text, &source_placeholders, &location)?;
            }
            Ok(())
        }
    }
}

fn validate_template_text(
    text: &str,
    source_placeholders: &BTreeSet<&str>,
    location: &str,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err(format!("{location} template text must not be empty"));
    }
    let placeholders =
        placeholder_names(text).map_err(|error| format!("invalid {location}: {error}"))?;
    validate_placeholder_set(source_placeholders, &placeholders, location)
}

fn placeholder_names(template: &str) -> Result<BTreeSet<&str>, FormatError> {
    Ok(format::template_parts(template)?
        .into_iter()
        .filter_map(|part| match part {
            format::TemplatePart::Text(_) => None,
            format::TemplatePart::Placeholder(name) => Some(name),
        })
        .collect())
}

fn validate_placeholder_set(
    source: &BTreeSet<&str>,
    translated: &BTreeSet<&str>,
    location: &str,
) -> Result<(), String> {
    let missing = source.difference(translated).copied().collect::<Vec<_>>();
    let extra = translated.difference(source).copied().collect::<Vec<_>>();
    if missing.is_empty() && extra.is_empty() {
        return Ok(());
    }

    let mut differences = Vec::with_capacity(2);
    if !missing.is_empty() {
        differences.push(format!("missing placeholders {missing:?}"));
    }
    if !extra.is_empty() {
        differences.push(format!("extra placeholders {extra:?}"));
    }
    Err(format!(
        "placeholder mismatch in {location}: {}",
        differences.join("; ")
    ))
}

impl Serialize for Catalog {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut entries = Vec::with_capacity(self.entries.len());
        for (id, entry) in &self.entries {
            if id.namespace != self.namespace {
                return Err(S::Error::custom(format!(
                    "catalog entry {:?} does not belong to namespace {:?}",
                    id, self.namespace
                )));
            }
            entries.push(CatalogEntryWire {
                id: id.clone(),
                source: entry.source.clone(),
                template: entry.template.clone(),
            });
        }

        CatalogWire {
            locale: self.locale.clone(),
            namespace: self.namespace.clone(),
            entries,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Catalog {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CatalogWire::deserialize(deserializer)?;
        let mut entries = BTreeMap::new();

        for item in wire.entries {
            if item.id.namespace != wire.namespace {
                return Err(D::Error::custom(format!(
                    "catalog entry {:?} does not belong to namespace {:?}",
                    item.id, wire.namespace
                )));
            }
            if entries.contains_key(&item.id) {
                return Err(D::Error::custom(format!(
                    "duplicate catalog entry {:?} in namespace {:?}",
                    item.id, wire.namespace
                )));
            }
            validate_entry(&item.source, &item.template).map_err(|reason| {
                D::Error::custom(format!("invalid catalog entry {:?}: {reason}", item.id))
            })?;
            entries.insert(
                item.id,
                CatalogEntry {
                    source: item.source,
                    template: item.template,
                },
            );
        }

        Ok(Self {
            locale: wire.locale,
            namespace: wire.namespace,
            entries,
        })
    }
}

impl Catalog {
    /// Parses and validates one catalog from its JSON representation.
    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serializes this catalog as human-readable JSON.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn new(locale: LocaleId, namespace: impl Into<String>) -> Self {
        Self {
            locale,
            namespace: namespace.into(),
            entries: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, entry: CatalogEntry) -> Option<CatalogEntry> {
        self.entries
            .insert(TextId::new(self.namespace.clone(), key), entry)
    }

    pub fn insert_with_context(
        &mut self,
        key: impl Into<String>,
        context: impl Into<String>,
        entry: CatalogEntry,
    ) -> Option<CatalogEntry> {
        self.entries.insert(
            TextId::new(self.namespace.clone(), key).with_context(context),
            entry,
        )
    }

    /// Looks up an exact key/context pair without crossing namespaces.
    #[must_use]
    pub fn get(&self, id: &TextId) -> Option<&CatalogEntry> {
        if id.namespace != self.namespace {
            return None;
        }
        self.entries.get(id)
    }
}

fn load_catalog_paths(paths: &[PathBuf]) -> Result<Vec<Catalog>, String> {
    let mut catalogs = Vec::with_capacity(paths.len());
    for path in paths {
        let json = std::fs::read_to_string(path)
            .map_err(|error| format!("failed to read catalog {}: {error}", path.display()))?;
        let catalog = Catalog::from_json_str(&json)
            .map_err(|error| format!("failed to parse catalog {}: {error}", path.display()))?;
        catalogs.push(catalog);
    }
    Ok(catalogs)
}

/// Runtime locale configuration and catalog sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct I18nConfig {
    pub default_language: LocaleId,
    pub fallback_language: LocaleId,
    #[serde(default)]
    pub catalog_paths: Vec<PathBuf>,
}

impl I18nConfig {
    /// Loads every configured catalog in declaration order.
    pub fn load_catalogs(&self) -> Result<CatalogSet, String> {
        let catalogs = load_catalog_paths(&self.catalog_paths)?;
        Ok(CatalogSet::new(
            self.default_language.clone(),
            self.fallback_language.clone(),
            catalogs,
        ))
    }
}

impl Default for I18nConfig {
    fn default() -> Self {
        let language = LocaleId::parse("en").expect("English is a valid ICU locale");
        Self {
            default_language: language.clone(),
            fallback_language: language,
            catalog_paths: Vec::new(),
        }
    }
}

/// Ordered catalogs used by the resolver.
///
/// Locale specificity is considered before list order; list order breaks ties
/// between catalogs with the same locale and namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogSet {
    pub default_language: LocaleId,
    pub fallback_language: LocaleId,
    pub catalogs: Vec<Catalog>,
}

impl CatalogSet {
    /// Loads catalogs using the default language configuration.
    ///
    /// Use [`I18nConfig::load_catalogs`] when the default or fallback language
    /// must come from application configuration.
    pub fn load(paths: &[PathBuf]) -> Result<Self, String> {
        let config = I18nConfig::default();
        let catalogs = load_catalog_paths(paths)?;
        Ok(Self::new(
            config.default_language,
            config.fallback_language,
            catalogs,
        ))
    }

    pub fn new(
        default_language: LocaleId,
        fallback_language: LocaleId,
        catalogs: Vec<Catalog>,
    ) -> Self {
        Self {
            default_language,
            fallback_language,
            catalogs,
        }
    }

    pub fn from_config(config: I18nConfig, catalogs: Vec<Catalog>) -> Self {
        Self::new(config.default_language, config.fallback_language, catalogs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i18n_config_defaults_to_english_without_catalog_paths() {
        let config = I18nConfig::default();
        assert_eq!(config.default_language, LocaleId::parse("en").unwrap());
        assert_eq!(config.fallback_language, LocaleId::parse("en").unwrap());
        assert!(config.catalog_paths.is_empty());
    }
}
