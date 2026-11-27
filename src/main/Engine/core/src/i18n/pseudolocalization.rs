use serde::{Deserialize, Serialize};

use crate::i18n::{FormatError, format};

const EXPANSION_FILLER: char = '~';
const RIGHT_TO_LEFT_MARK: char = '\u{200f}';
const MAX_EXPANSION_RATIO: f32 = 10.0;
const MAX_ADDITIONAL_CHARACTERS: usize = 4096;

/// Controls how source text is transformed for pseudolocale previews.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PseudolocalizationConfig {
    /// Target length of literal text relative to the source, excluding
    /// placeholders, prefix, and suffix. Values at or below `1.0` do not shrink
    /// text; non-finite values are ignored. Expansion is capped at `10x` the
    /// source length and at 4096 additional characters.
    pub expansion_ratio: f32,
    pub prefix: String,
    pub suffix: String,
    pub replace_accents: bool,
    pub fake_bidi: bool,
}

impl Default for PseudolocalizationConfig {
    fn default() -> Self {
        Self {
            expansion_ratio: 1.3,
            prefix: "[[".to_owned(),
            suffix: "]]".to_owned(),
            replace_accents: true,
            fake_bidi: true,
        }
    }
}

impl PseudolocalizationConfig {
    /// Pseudolocalizes one template using this configuration.
    pub fn pseudolocalize_template(&self, template: &str) -> Result<String, FormatError> {
        pseudolocalize_template(template, self)
    }
}

/// Transforms literal text while preserving named placeholders verbatim.
///
/// Expansion is measured in Unicode scalar values and adds `~` characters after
/// the transformed message. It is capped at `10x` the source length and at 4096
/// additional characters. Prefix and suffix are not expanded or accented.
/// Invalid placeholder syntax is reported using the same errors as formatting.
pub fn pseudolocalize_template(
    template: &str,
    config: &PseudolocalizationConfig,
) -> Result<String, FormatError> {
    let parts = format::template_parts(template)?;
    let source_characters = parts
        .iter()
        .map(|part| match part {
            format::TemplatePart::Text(text) => text.chars().count(),
            format::TemplatePart::Placeholder(_) => 0,
        })
        .sum::<usize>();
    let target_characters = expanded_text_length(source_characters, config.expansion_ratio);

    let mut body = String::with_capacity(template.len());
    for part in parts {
        match part {
            format::TemplatePart::Text(text) => {
                for character in text.chars() {
                    body.push(if config.replace_accents {
                        replace_with_accent(character)
                    } else {
                        character
                    });
                }
            }
            format::TemplatePart::Placeholder(name) => {
                body.push('{');
                body.push_str(name);
                body.push('}');
            }
        }
    }

    for _ in 0..target_characters.saturating_sub(source_characters) {
        body.push(EXPANSION_FILLER);
    }

    let mut output =
        String::with_capacity(body.len() + config.prefix.len() + config.suffix.len() + 2);
    if config.fake_bidi {
        output.push(RIGHT_TO_LEFT_MARK);
    }
    output.push_str(&config.prefix);
    output.push_str(&body);
    output.push_str(&config.suffix);
    if config.fake_bidi {
        output.push(RIGHT_TO_LEFT_MARK);
    }
    Ok(output)
}

fn expanded_text_length(character_count: usize, ratio: f32) -> usize {
    if !ratio.is_finite() || ratio <= 1.0 {
        return character_count;
    }

    let ratio_capped_target =
        ((character_count as f32) * ratio.min(MAX_EXPANSION_RATIO)).ceil() as usize;
    let ratio_capped_target = ratio_capped_target.max(character_count);
    character_count
        .saturating_add(MAX_ADDITIONAL_CHARACTERS)
        .min(ratio_capped_target)
}

fn replace_with_accent(character: char) -> char {
    match character {
        'a' => 'á',
        'e' => 'é',
        'i' => 'í',
        'o' => 'ó',
        'u' => 'ú',
        'A' => 'Á',
        'E' => 'É',
        'I' => 'Í',
        'O' => 'Ó',
        'U' => 'Ú',
        other => other,
    }
}
