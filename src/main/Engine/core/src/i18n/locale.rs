use std::fmt;
use std::str::FromStr;

use icu_locale::Locale;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::i18n::LocaleParseError;

/// A normalized BCP-47 locale backed by ICU4X.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LocaleId(Locale);

impl LocaleId {
    /// Parses a locale, accepting underscores as separators for compatibility
    /// with common configuration files.
    pub fn parse(input: &str) -> Result<Self, LocaleParseError> {
        let normalized = input.trim().replace('_', "-");
        if normalized.is_empty() {
            return Err(LocaleParseError {
                input: input.to_owned(),
                message: "the locale is empty".to_owned(),
            });
        }

        normalized
            .parse::<Locale>()
            .map(Self)
            .map_err(|error| LocaleParseError {
                input: input.to_owned(),
                message: error.to_string(),
            })
    }

    /// Wraps an already parsed ICU4X locale.
    #[must_use]
    pub fn new(locale: Locale) -> Self {
        Self(locale)
    }

    /// Returns the underlying ICU4X locale.
    #[must_use]
    pub const fn as_icu(&self) -> &Locale {
        &self.0
    }

    /// Consumes the wrapper and returns the ICU4X locale.
    #[must_use]
    pub fn into_inner(self) -> Locale {
        self.0
    }

    /// Returns the lowercase language subtag.
    #[must_use]
    pub fn language(&self) -> &str {
        self.0.id.language.as_str()
    }

    /// Returns the title-case script subtag, when present.
    #[must_use]
    pub fn script(&self) -> Option<&str> {
        self.0.id.script.as_ref().map(|script| script.as_str())
    }

    /// Returns the uppercase region subtag, when present.
    #[must_use]
    pub fn region(&self) -> Option<&str> {
        self.0.id.region.as_ref().map(|region| region.as_str())
    }

    /// Returns the locale variants in their canonical order.
    #[must_use]
    pub fn variants(&self) -> Vec<&str> {
        self.0
            .id
            .variants
            .iter()
            .map(|variant| variant.as_str())
            .collect()
    }

    /// Returns progressively less specific locales, starting with this one.
    ///
    /// Unicode extensions are removed after the exact identifier. Variants,
    /// region and script are then removed in that order. For example,
    /// `zh-Hans-CN` yields `zh-Hans-CN`, `zh-Hans`, and `zh`.
    #[must_use]
    pub fn fallback_candidates(&self) -> Vec<Self> {
        let language = self.language();
        let script = self.script();
        let region = self.region();
        let variants = self.variants();

        let mut identifiers = vec![self.to_string()];
        identifiers.push(join_locale(language, script, region, &variants));

        if !variants.is_empty() {
            identifiers.push(join_locale(language, script, region, &[]));
        }
        if region.is_some() {
            identifiers.push(join_locale(language, script, None, &[]));
        }
        if script.is_some() {
            identifiers.push(join_locale(language, None, None, &[]));
        }
        identifiers.push(language.to_owned());

        let mut candidates = Vec::with_capacity(identifiers.len());
        for identifier in identifiers {
            let candidate = Self::parse(&identifier)
                .expect("locale fallback candidates are built from a valid parsed locale");
            if !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
        candidates
    }
}

fn join_locale(
    language: &str,
    script: Option<&str>,
    region: Option<&str>,
    variants: &[&str],
) -> String {
    let mut parts = vec![language];
    parts.extend(script);
    parts.extend(region);
    parts.extend(variants.iter().copied());
    parts.join("-")
}

impl FromStr for LocaleId {
    type Err = LocaleParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl TryFrom<&str> for LocaleId {
    type Error = LocaleParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<String> for LocaleId {
    type Error = LocaleParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Locale> for LocaleId {
    fn from(locale: Locale) -> Self {
        Self::new(locale)
    }
}

impl AsRef<Locale> for LocaleId {
    fn as_ref(&self) -> &Locale {
        self.as_icu()
    }
}

impl fmt::Display for LocaleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Serialize for LocaleId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for LocaleId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}
