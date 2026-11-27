use std::collections::BTreeMap;
use std::str::FromStr;

use fixed_decimal::Decimal;
use icu_plurals::PluralRules;
use serde_json::Value;

use crate::i18n::{
    FormatError, LocaleId, LookupResult, MessageTemplate, PluralCategory, TextArguments,
};

/// Formats a lookup result independently from catalog selection.
pub fn format_lookup(
    lookup: &LookupResult,
    arguments: Option<&TextArguments>,
) -> Result<String, FormatError> {
    let empty = BTreeMap::new();
    format_template(
        &lookup.template,
        &lookup.locale,
        arguments.unwrap_or(&empty),
    )
}

/// Formats a simple or plural message using ICU4X cardinal plural rules.
///
/// Plural templates reserve the named argument `count`. Both JSON numbers and
/// numeric strings are accepted so visible decimal fractions participate in
/// plural selection (for example, English `1.0` selects `other`).
pub fn format_template(
    template: &MessageTemplate,
    locale: &LocaleId,
    arguments: &TextArguments,
) -> Result<String, FormatError> {
    let text = if let Some(forms) = &template.plural_forms {
        let count = arguments
            .get("count")
            .ok_or(FormatError::MissingPluralArgument)?;
        let number = parse_plural_number(count)?;
        let rules =
            PluralRules::try_new_cardinal(locale.as_icu().clone().into()).map_err(|error| {
                FormatError::PluralRules {
                    locale: locale.to_string(),
                    reason: error.to_string(),
                }
            })?;
        let category = PluralCategory::from(rules.category_for(&number));
        forms
            .get(&category)
            .or_else(|| forms.get(&PluralCategory::Other))
            .ok_or_else(|| FormatError::MissingPluralForm {
                locale: locale.to_string(),
                category: category.to_string(),
            })?
    } else {
        template.text.as_deref().ok_or(FormatError::EmptyTemplate)?
    };

    render_placeholders(text, arguments)
}

fn parse_plural_number(value: &Value) -> Result<Decimal, FormatError> {
    let number = match value {
        Value::Number(number) => number.to_string(),
        Value::String(number) => number.clone(),
        other => {
            return Err(FormatError::InvalidPluralNumber {
                value: other.to_string(),
                reason: "expected a JSON number or numeric string".to_owned(),
            });
        }
    };

    Decimal::from_str(&number).map_err(|error| FormatError::InvalidPluralNumber {
        value: number.clone(),
        reason: error.to_string(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TemplatePart<'a> {
    Text(&'a str),
    Placeholder(&'a str),
}

/// Splits a template into literal text and validated placeholder names.
///
/// Placeholder names are returned without their braces. Both catalog validation
/// and pseudolocalization use this parser so they accept exactly the same syntax
/// as runtime formatting.
pub(crate) fn template_parts(template: &str) -> Result<Vec<TemplatePart<'_>>, FormatError> {
    let bytes = template.as_bytes();
    let mut parts = Vec::new();
    let mut text_start = 0;
    let mut position = 0;

    while position < bytes.len() {
        match bytes[position] {
            b'{' => {
                if text_start < position {
                    parts.push(TemplatePart::Text(&template[text_start..position]));
                }

                let opening = position;
                position += 1;
                let name_start = position;
                while position < bytes.len() && bytes[position] != b'}' && bytes[position] != b'{' {
                    position += 1;
                }

                if position == bytes.len() {
                    return Err(FormatError::UnclosedPlaceholder { position: opening });
                }
                if bytes[position] == b'{' {
                    return Err(FormatError::InvalidPlaceholder {
                        position: opening,
                        reason: "nested placeholders are not supported".to_owned(),
                    });
                }

                let name = &template[name_start..position];
                validate_placeholder_name(name, opening)?;
                parts.push(TemplatePart::Placeholder(name));
                position += 1;
                text_start = position;
            }
            b'}' => {
                return Err(FormatError::UnexpectedClosingBrace { position });
            }
            _ => {
                let character = template[position..]
                    .chars()
                    .next()
                    .expect("position is always on a UTF-8 character boundary");
                position += character.len_utf8();
            }
        }
    }

    if text_start < bytes.len() {
        parts.push(TemplatePart::Text(&template[text_start..]));
    }

    Ok(parts)
}

fn render_placeholders(template: &str, arguments: &TextArguments) -> Result<String, FormatError> {
    let mut output = String::with_capacity(template.len());

    for part in template_parts(template)? {
        match part {
            TemplatePart::Text(text) => output.push_str(text),
            TemplatePart::Placeholder(name) => {
                let value = arguments
                    .get(name)
                    .ok_or_else(|| FormatError::MissingArgument {
                        name: name.to_owned(),
                    })?;
                output.push_str(&argument_display(value));
            }
        }
    }

    Ok(output)
}

fn validate_placeholder_name(name: &str, position: usize) -> Result<(), FormatError> {
    let mut characters = name.chars();
    let first = characters
        .next()
        .ok_or_else(|| FormatError::InvalidPlaceholder {
            position,
            reason: "the placeholder name is empty".to_owned(),
        })?;

    let valid = (first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric());
    if valid {
        Ok(())
    } else {
        Err(FormatError::InvalidPlaceholder {
            position,
            reason: format!("{name:?} is not a named identifier"),
        })
    }
}

fn argument_display(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".to_owned(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}
