use std::fmt;

/// Error produced when a string cannot be parsed as a BCP-47 locale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleParseError {
    pub input: String,
    pub message: String,
}

impl fmt::Display for LocaleParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid locale {:?}: {}",
            self.input, self.message
        )
    }
}

impl std::error::Error for LocaleParseError {}

/// Errors raised while selecting or formatting a message.
///
/// Lookup itself is infallible: a missing message falls back to its source
/// string. These errors describe invalid templates or arguments instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    EmptyTemplate,
    MissingPluralArgument,
    InvalidPluralNumber { value: String, reason: String },
    MissingPluralForm { locale: String, category: String },
    PluralRules { locale: String, reason: String },
    MissingArgument { name: String },
    InvalidPlaceholder { position: usize, reason: String },
    UnclosedPlaceholder { position: usize },
    UnexpectedClosingBrace { position: usize },
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTemplate => {
                formatter.write_str("message template has neither text nor plural forms")
            }
            Self::MissingPluralArgument => formatter.write_str(
                "plural message requires the named argument {count} to select a plural form",
            ),
            Self::InvalidPluralNumber { value, reason } => write!(
                formatter,
                "invalid plural value for {{count}} ({value:?}): {reason}"
            ),
            Self::MissingPluralForm { locale, category } => write!(
                formatter,
                "locale {locale:?} has no plural form for category {category:?} and no 'other' fallback"
            ),
            Self::PluralRules { locale, reason } => {
                write!(
                    formatter,
                    "cannot load plural rules for {locale:?}: {reason}"
                )
            }
            Self::MissingArgument { name } => {
                write!(formatter, "missing value for placeholder {{{name}}}")
            }
            Self::InvalidPlaceholder { position, reason } => write!(
                formatter,
                "invalid placeholder at byte {position}: {reason}; expected a named placeholder such as {{name}}"
            ),
            Self::UnclosedPlaceholder { position } => write!(
                formatter,
                "unclosed placeholder at byte {position}: missing closing '}}'"
            ),
            Self::UnexpectedClosingBrace { position } => write!(
                formatter,
                "unexpected '}}' at byte {position}: placeholders must be opened with '{{'"
            ),
        }
    }
}

impl std::error::Error for FormatError {}
