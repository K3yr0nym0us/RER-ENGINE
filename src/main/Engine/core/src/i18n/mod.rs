mod catalog;
mod error;
mod format;
mod locale;
mod lookup;
mod pseudolocalization;
mod types;

#[cfg(test)]
mod tests;

pub use catalog::{Catalog, CatalogSet, I18nConfig};
pub use error::{FormatError, LocaleParseError};
pub use locale::LocaleId;
pub use lookup::LookupResult;
pub use pseudolocalization::{PseudolocalizationConfig, pseudolocalize_template};
pub use types::{
    CatalogEntry, LocalizedText, MessageTemplate, PluralCategory, ResolvedText, TextArguments,
    TextId, TextValue,
};

/// Separately exposed formatting helpers for callers that cache lookup results.
pub mod formatting {
    pub use crate::i18n::format::{format_lookup, format_template};
    pub use crate::i18n::pseudolocalization::pseudolocalize_template;
}
