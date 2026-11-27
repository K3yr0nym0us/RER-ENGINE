use std::collections::BTreeMap;
use std::fmt;

use icu_plurals::PluralCategory as IcuPluralCategory;
use serde::Serialize;
use serde_json::Value;

use crate::i18n::LocaleId;

/// Named values available to message placeholders and plural selection.
pub type TextArguments = BTreeMap<String, Value>;

/// Stable identity of a message within a namespace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextId {
    pub namespace: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

impl TextId {
    pub fn new(namespace: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            key: key.into(),
            context: None,
        }
    }

    #[must_use]
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

/// Either a catalog-backed localized message or an unchanged literal string.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TextValue {
    Localized(LocalizedText),
    Literal { value: String },
}

impl TextValue {
    #[must_use]
    pub fn literal(value: impl Into<String>) -> Self {
        Self::Literal {
            value: value.into(),
        }
    }

    /// Texto fuente, tanto para valores localizados como literales.
    #[must_use]
    pub fn source_text(&self) -> &str {
        match self {
            Self::Localized(text) => &text.source,
            Self::Literal { value } => value,
        }
    }

    /// Acceso al contenido cuando el valor es literal.
    #[must_use]
    pub fn as_literal(&self) -> Option<&str> {
        match self {
            Self::Literal { value } => Some(value),
            Self::Localized(_) => None,
        }
    }

    /// Acceso mutable al contenido cuando el valor es literal.
    #[must_use]
    pub fn as_literal_mut(&mut self) -> Option<&mut String> {
        match self {
            Self::Literal { value } => Some(value),
            Self::Localized(_) => None,
        }
    }

    /// Indica si el valor depende de un catálogo de traducción.
    #[must_use]
    pub fn is_localized(&self) -> bool {
        matches!(self, Self::Localized(_))
    }
}

/// Runtime input for a localized message.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalizedText {
    pub id: TextId,
    /// Source-language text used when no catalog entry exists. It is never
    /// replaced by the message key.
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<TextArguments>,
}

impl LocalizedText {
    pub fn new(id: TextId, source: impl Into<String>) -> Self {
        Self {
            id,
            source: source.into(),
            arguments: None,
        }
    }

    #[must_use]
    pub fn with_arguments(mut self, arguments: TextArguments) -> Self {
        self.arguments = Some(arguments);
        self
    }
}

/// ICU cardinal plural categories.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl From<IcuPluralCategory> for PluralCategory {
    fn from(category: IcuPluralCategory) -> Self {
        match category {
            IcuPluralCategory::Zero => Self::Zero,
            IcuPluralCategory::One => Self::One,
            IcuPluralCategory::Two => Self::Two,
            IcuPluralCategory::Few => Self::Few,
            IcuPluralCategory::Many => Self::Many,
            IcuPluralCategory::Other => Self::Other,
        }
    }
}

impl fmt::Display for PluralCategory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Zero => "zero",
            Self::One => "one",
            Self::Two => "two",
            Self::Few => "few",
            Self::Many => "many",
            Self::Other => "other",
        })
    }
}

/// A simple message or a set of cardinal plural forms selected by `{count}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageTemplate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plural_forms: Option<BTreeMap<PluralCategory, String>>,
}

impl MessageTemplate {
    #[must_use]
    pub fn text(value: impl Into<String>) -> Self {
        Self {
            text: Some(value.into()),
            plural_forms: None,
        }
    }

    #[must_use]
    pub fn plural(forms: BTreeMap<PluralCategory, String>) -> Self {
        Self {
            text: None,
            plural_forms: Some(forms),
        }
    }
}

/// Source and translated template stored in a catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogEntry {
    pub source: String,
    pub template: MessageTemplate,
}

/// Final text plus metadata about the lookup and formatting result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedText {
    pub display: String,
    pub locale: LocaleId,
    pub source: String,
    /// True when no catalog entry existed or its recorded source differs from
    /// the source supplied by the caller.
    pub stale: bool,
}
