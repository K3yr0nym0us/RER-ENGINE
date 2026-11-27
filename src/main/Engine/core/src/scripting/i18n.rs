use crate::i18n::{CatalogSet, LocaleId, TextArguments, TextId, formatting};

const DEFAULT_NAMESPACE: &str = "game";

/// A script-visible i18n update that the host can apply after draining the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum I18nChange {
    LanguageChanged { language: LocaleId, epoch: u64 },
    LocaleChanged { locale: LocaleId, epoch: u64 },
}

/// Host-readable copy of the i18n state shared by every Rhai script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptI18nSnapshot {
    pub catalogs: CatalogSet,
    pub active_language: LocaleId,
    pub formatting_locale: LocaleId,
    pub epoch: u64,
}

/// Mutable i18n state captured by the native Rhai functions.
#[derive(Debug, Clone)]
pub struct ScriptI18nState {
    pub catalogs: CatalogSet,
    pub active_language: LocaleId,
    pub formatting_locale: LocaleId,
    pub epoch: u64,
    changes: Vec<I18nChange>,
}

impl Default for ScriptI18nState {
    fn default() -> Self {
        let language = LocaleId::parse("en").expect("the built-in English locale is valid");
        let catalogs = CatalogSet::new(language.clone(), language.clone(), Vec::new());
        Self::new(catalogs, language.clone(), language)
    }
}

impl ScriptI18nState {
    #[must_use]
    pub fn new(
        catalogs: CatalogSet,
        active_language: LocaleId,
        formatting_locale: LocaleId,
    ) -> Self {
        Self {
            catalogs,
            active_language,
            formatting_locale,
            epoch: 0,
            changes: Vec::new(),
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> ScriptI18nSnapshot {
        ScriptI18nSnapshot {
            catalogs: self.catalogs.clone(),
            active_language: self.active_language.clone(),
            formatting_locale: self.formatting_locale.clone(),
            epoch: self.epoch,
        }
    }

    /// Applies an authoritative host update without queuing feedback for that host.
    /// Pending script changes are discarded because the host state supersedes them.
    pub fn set_i18n(
        &mut self,
        catalogs: CatalogSet,
        active_language: LocaleId,
        formatting_locale: LocaleId,
    ) {
        let changed = self.catalogs != catalogs
            || self.active_language != active_language
            || self.formatting_locale != formatting_locale;

        self.catalogs = catalogs;
        self.active_language = active_language;
        self.formatting_locale = formatting_locale;
        self.changes.clear();
        if changed {
            self.epoch = self.epoch.wrapping_add(1);
        }
    }

    pub fn set_language(&mut self, language: LocaleId) {
        if self.active_language == language {
            return;
        }

        self.active_language = language.clone();
        self.epoch = self.epoch.wrapping_add(1);
        self.changes.push(I18nChange::LanguageChanged {
            language,
            epoch: self.epoch,
        });
    }

    pub fn set_locale(&mut self, locale: LocaleId) {
        if self.formatting_locale == locale {
            return;
        }

        self.formatting_locale = locale.clone();
        self.epoch = self.epoch.wrapping_add(1);
        self.changes.push(I18nChange::LocaleChanged {
            locale,
            epoch: self.epoch,
        });
    }

    pub fn drain_changes(&mut self) -> Vec<I18nChange> {
        std::mem::take(&mut self.changes)
    }

    /// Resolves a `game` message in the active language. Plural rules follow the
    /// matched catalog locale; `formatting_locale` remains a separate host setting.
    pub fn translate(&self, key: &str, source: Option<&str>) -> String {
        self.translate_with_arguments(key, &TextArguments::new(), source)
    }

    /// Resolves and formats a message with named placeholder/plural arguments.
    /// Plural rules follow the matched catalog locale, not `formatting_locale`.
    pub fn translate_with_arguments(
        &self,
        key: &str,
        arguments: &TextArguments,
        source: Option<&str>,
    ) -> String {
        let source = match source {
            Some(source) => source.to_owned(),
            None => match self.source_for_key(key) {
                Some(source) => source,
                None => {
                    log::warn!(
                        "[scripting] tr({key:?}): no source in the game fallback/default catalogs; returning an empty string"
                    );
                    return String::new();
                }
            },
        };

        let id = TextId::new(DEFAULT_NAMESPACE, key);
        let lookup = self
            .catalogs
            .lookup_source(&self.active_language, &id, &source);
        match formatting::format_lookup(&lookup, Some(arguments)) {
            Ok(translated) => translated,
            Err(error) => {
                log::warn!(
                    "[scripting] tr({key:?}) with catalog locale {}: {error}",
                    lookup.locale
                );
                String::new()
            }
        }
    }

    /// Resolves a plural message using `count` as its plural-selection argument.
    pub fn translate_plural(
        &self,
        key: &str,
        count: serde_json::Value,
        source: Option<&str>,
    ) -> String {
        let mut arguments = TextArguments::new();
        arguments.insert("count".to_owned(), count);
        self.translate_with_arguments(key, &arguments, source)
    }

    fn source_for_key(&self, key: &str) -> Option<String> {
        let id = TextId::new(DEFAULT_NAMESPACE, key);
        [
            &self.catalogs.fallback_language,
            &self.catalogs.default_language,
        ]
        .into_iter()
        .flat_map(LocaleId::fallback_candidates)
        .find_map(|candidate| {
            self.catalogs
                .catalogs
                .iter()
                .filter(|catalog| {
                    catalog.locale == candidate && catalog.namespace == DEFAULT_NAMESPACE
                })
                .find_map(|catalog| catalog.get(&id).map(|entry| entry.source.clone()))
        })
    }
}
