use serde::{Deserialize, Serialize};

use crate::i18n::{
    Catalog, CatalogEntry, CatalogSet, FormatError, LocaleId, MessageTemplate, ResolvedText,
    TextId, TextValue, format,
};

/// Result of catalog lookup before placeholder and plural formatting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupResult {
    pub locale: LocaleId,
    pub source: String,
    pub template: MessageTemplate,
    pub stale: bool,
}

impl CatalogSet {
    /// Looks up one message. Locale fallback is performed independently for
    /// every key, so a partially translated locale still supplies its available
    /// messages.
    #[must_use]
    pub fn lookup(
        &self,
        requested_locale: &LocaleId,
        text: &crate::i18n::LocalizedText,
    ) -> LookupResult {
        self.lookup_source(requested_locale, &text.id, &text.source)
    }

    /// Looks up an identity and source string without requiring runtime
    /// arguments. Missing entries use the source string as a literal template.
    #[must_use]
    pub fn lookup_source(
        &self,
        requested_locale: &LocaleId,
        id: &TextId,
        source: &str,
    ) -> LookupResult {
        if let Some((catalog, entry)) = self.find_message(requested_locale, id) {
            return LookupResult {
                locale: catalog.locale.clone(),
                source: source.to_owned(),
                template: entry.template.clone(),
                stale: entry.source != source,
            };
        }

        if let Some((catalog, entry)) = self.find_message(&self.fallback_language, id) {
            return LookupResult {
                locale: catalog.locale.clone(),
                source: source.to_owned(),
                template: entry.template.clone(),
                stale: entry.source != source,
            };
        }

        LookupResult {
            locale: self.fallback_language.clone(),
            source: source.to_owned(),
            template: MessageTemplate::text(source),
            stale: true,
        }
    }

    fn find_message<'a>(
        &'a self,
        requested_locale: &LocaleId,
        id: &TextId,
    ) -> Option<(&'a Catalog, &'a CatalogEntry)> {
        for candidate in requested_locale.fallback_candidates() {
            if let Some(found) = self.find_exact_locale(&candidate, id) {
                return Some(found);
            }
        }
        None
    }

    fn find_exact_locale<'a>(
        &'a self,
        locale: &LocaleId,
        id: &TextId,
    ) -> Option<(&'a Catalog, &'a CatalogEntry)> {
        self.catalogs
            .iter()
            .filter(|catalog| catalog.namespace == id.namespace && catalog.locale == *locale)
            .find_map(|catalog| catalog.get(id).map(|entry| (catalog, entry)))
    }

    /// Formats a previously selected lookup result. Keeping this separate lets
    /// callers cache lookup metadata and format repeatedly with new arguments.
    pub fn format_lookup(
        lookup: &LookupResult,
        arguments: Option<&crate::i18n::TextArguments>,
    ) -> Result<String, FormatError> {
        format::format_lookup(lookup, arguments)
    }

    /// Resolves and formats a message in the requested locale.
    pub fn resolve(
        &self,
        requested_locale: &LocaleId,
        text: &crate::i18n::LocalizedText,
    ) -> Result<ResolvedText, FormatError> {
        let lookup = self.lookup(requested_locale, text);
        let display = Self::format_lookup(&lookup, text.arguments.as_ref())?;
        Ok(ResolvedText {
            display,
            locale: lookup.locale,
            source: lookup.source,
            stale: lookup.stale,
        })
    }

    /// Resolves a message using [`CatalogSet::default_language`].
    pub fn resolve_default(
        &self,
        text: &crate::i18n::LocalizedText,
    ) -> Result<ResolvedText, FormatError> {
        self.resolve(&self.default_language, text)
    }

    /// Resolves either a localized message or a literal value.
    pub fn resolve_value(
        &self,
        requested_locale: &LocaleId,
        value: &TextValue,
    ) -> Result<ResolvedText, FormatError> {
        match value {
            TextValue::Localized(text) => self.resolve(requested_locale, text),
            TextValue::Literal { value } => Ok(ResolvedText {
                display: value.clone(),
                locale: requested_locale.clone(),
                source: value.clone(),
                stale: false,
            }),
        }
    }

    /// Resolves either variant using [`CatalogSet::default_language`].
    pub fn resolve_default_value(&self, value: &TextValue) -> Result<ResolvedText, FormatError> {
        match value {
            TextValue::Localized(text) => self.resolve_default(text),
            TextValue::Literal { value } => Ok(ResolvedText {
                display: value.clone(),
                locale: self.default_language.clone(),
                source: value.clone(),
                stale: false,
            }),
        }
    }
}
