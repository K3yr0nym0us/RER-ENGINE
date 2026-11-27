use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};

use crate::i18n::{
    Catalog, CatalogEntry, CatalogSet, FormatError, I18nConfig, LocaleId, LocalizedText,
    MessageTemplate, PluralCategory, PseudolocalizationConfig, TextArguments, TextId, TextValue,
    pseudolocalize_template,
};

fn locale(value: &str) -> LocaleId {
    LocaleId::parse(value).expect("test locale should parse")
}

fn localized(namespace: &str, key: &str, source: &str) -> LocalizedText {
    LocalizedText::new(TextId::new(namespace, key), source)
}

fn entry(source: &str, template: MessageTemplate) -> CatalogEntry {
    CatalogEntry {
        source: source.to_owned(),
        template,
    }
}

fn arguments(values: &[(&str, Value)]) -> TextArguments {
    values
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect()
}

struct TestCatalogDirectory {
    path: PathBuf,
}

impl TestCatalogDirectory {
    fn new(label: &str) -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rer-engine-i18n-{label}-{}-{id}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("test directory should be created");
        Self { path }
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.path.join(name);
        fs::write(&path, contents).expect("test catalog should be written");
        path
    }
}

impl Drop for TestCatalogDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn catalog_document(template: Value) -> String {
    catalog_document_with_source("en", "Hello", template)
}

fn catalog_document_with_source(locale: &str, source: &str, template: Value) -> String {
    json!({
        "locale": locale,
        "namespace": "ui",
        "entries": [
            {
                "id": {"namespace": "ui", "key": "hello"},
                "source": source,
                "template": template
            }
        ]
    })
    .to_string()
}

fn plural(forms: &[(PluralCategory, &str)]) -> MessageTemplate {
    MessageTemplate::plural(
        forms
            .iter()
            .map(|(category, text)| (*category, (*text).to_owned()))
            .collect(),
    )
}

#[test]
fn locale_normalizes_bcp47_and_underscores() {
    let value = locale("  ZH_hAnS_cn  ");
    assert_eq!(value.to_string(), "zh-Hans-CN");
    assert_eq!(value.language(), "zh");
    assert_eq!(value.script(), Some("Hans"));
    assert_eq!(value.region(), Some("CN"));
    assert!(value.variants().is_empty());

    let variant = locale("DE_1901");
    assert_eq!(variant.to_string(), "de-1901");
    assert_eq!(variant.variants(), vec!["1901"]);
}

#[test]
fn locale_rejects_empty_and_malformed_identifiers() {
    for invalid in ["", "   ", "en--US", "123", "en US"] {
        assert!(LocaleId::parse(invalid).is_err(), "accepted {invalid:?}");
    }
}

#[test]
fn locale_fallback_candidates_preserve_script_and_drop_variants_in_order() {
    assert_eq!(
        locale("zh-Hans-CN")
            .fallback_candidates()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["zh-Hans-CN", "zh-Hans", "zh"]
    );
    assert_eq!(
        locale("de-1901")
            .fallback_candidates()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["de-1901", "de"]
    );
    assert_eq!(
        locale("en-US-u-ca-gregory")
            .fallback_candidates()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["en-US-u-ca-gregory", "en-US", "en"]
    );
}

#[test]
fn locale_serde_is_a_normalized_string() {
    let serialized = serde_json::to_string(&locale("ES_MX")).unwrap();
    assert_eq!(serialized, "\"es-MX\"");
    assert_eq!(
        serde_json::from_str::<LocaleId>("\"pt_BR\"").unwrap(),
        locale("pt-BR")
    );
    assert!(serde_json::from_str::<LocaleId>("\"en--US\"").is_err());
}

#[test]
fn text_id_and_localized_text_use_camel_case_and_omit_none() {
    let id = TextId::new("player_ui", "save").with_context("toolbar");
    assert_eq!(
        serde_json::to_value(&id).unwrap(),
        json!({"namespace": "player_ui", "key": "save", "context": "toolbar"})
    );
    assert_eq!(
        serde_json::to_value(TextId::new("player_ui", "save")).unwrap(),
        json!({"namespace": "player_ui", "key": "save"})
    );

    let text = LocalizedText::new(id, "Save").with_arguments(arguments(&[("count", json!(2))]));
    assert_eq!(
        serde_json::to_value(text).unwrap(),
        json!({
            "id": {"namespace": "player_ui", "key": "save", "context": "toolbar"},
            "source": "Save",
            "arguments": {"count": 2}
        })
    );
}

#[test]
fn text_value_is_internally_tagged_with_camel_case_kind() {
    let localized_value = TextValue::Localized(localized("ui", "hello", "Hello"));
    assert_eq!(
        serde_json::to_value(localized_value).unwrap(),
        json!({
            "kind": "localized",
            "id": {"namespace": "ui", "key": "hello"},
            "source": "Hello"
        })
    );

    let literal = TextValue::Literal {
        value: "Untranslated".to_owned(),
    };
    assert_eq!(
        serde_json::to_value(literal.clone()).unwrap(),
        json!({"kind": "literal", "value": "Untranslated"})
    );
    assert_eq!(
        serde_json::from_value::<TextValue>(serde_json::to_value(literal).unwrap()).unwrap(),
        TextValue::literal("Untranslated")
    );
}

#[test]
fn catalog_and_config_serde_have_no_version_fields() {
    let mut catalog = Catalog::new(locale("en"), "ui");
    catalog.insert("hello", entry("Hello", MessageTemplate::text("Hello")));
    let set = CatalogSet::new(locale("en"), locale("es"), vec![catalog]);
    let serialized = serde_json::to_string(&set).unwrap();
    assert!(!serialized.contains("version"));
    assert!(!serialized.contains("schemaVersion"));
    assert_eq!(
        serde_json::from_str::<CatalogSet>(&serialized).unwrap(),
        set
    );

    let config = crate::i18n::I18nConfig {
        default_language: locale("en"),
        fallback_language: locale("es"),
        catalog_paths: vec!["catalogs/en.json".into()],
    };
    let value = serde_json::to_value(config).unwrap();
    assert_eq!(value["defaultLanguage"], "en");
    assert_eq!(value["fallbackLanguage"], "es");
    assert_eq!(value["catalogPaths"], json!(["catalogs/en.json"]));
    assert!(value.get("version").is_none());
}

#[test]
fn catalog_json_uses_explicit_readable_entries_and_round_trips() {
    let mut catalog = Catalog::new(locale("en"), "ui");
    catalog.insert("open", entry("Open", MessageTemplate::text("Open")));
    catalog.insert_with_context(
        "open",
        "menu",
        entry("Open", MessageTemplate::text("Open menu")),
    );

    let value = serde_json::to_value(&catalog).unwrap();
    assert_eq!(
        value,
        json!({
            "locale": "en",
            "namespace": "ui",
            "entries": [
                {
                    "id": {"namespace": "ui", "key": "open"},
                    "source": "Open",
                    "template": {"text": "Open"}
                },
                {
                    "id": {"namespace": "ui", "key": "open", "context": "menu"},
                    "source": "Open",
                    "template": {"text": "Open menu"}
                }
            ]
        })
    );

    let restored: Catalog = serde_json::from_value(value).unwrap();
    assert_eq!(restored, catalog);
    assert_eq!(restored.entries.len(), 2);
    assert_eq!(
        restored
            .get(&TextId::new("ui", "open").with_context("menu"))
            .unwrap()
            .template,
        MessageTemplate::text("Open menu")
    );
}

#[test]
fn catalog_deserialization_rejects_ambiguous_duplicate_ids() {
    let value = json!({
        "locale": "en",
        "namespace": "ui",
        "entries": [
            {
                "id": {"namespace": "ui", "key": "open", "context": "menu"},
                "source": "Open",
                "template": {"text": "Open menu"}
            },
            {
                "id": {"namespace": "ui", "key": "open", "context": "menu"},
                "source": "Changed source",
                "template": {"text": "Different translation"}
            }
        ]
    });

    let error = serde_json::from_value::<Catalog>(value)
        .expect_err("duplicate ids must be rejected")
        .to_string();
    assert!(error.contains("duplicate catalog entry"), "{error}");
}

#[test]
fn catalog_deserialization_rejects_entries_from_another_namespace() {
    let value = json!({
        "locale": "en",
        "namespace": "ui",
        "entries": [
            {
                "id": {"namespace": "gameplay", "key": "open"},
                "source": "Open",
                "template": {"text": "Open"}
            }
        ]
    });

    let error = serde_json::from_value::<Catalog>(value)
        .expect_err("mismatched namespaces must be rejected")
        .to_string();
    assert!(error.contains("does not belong to namespace"), "{error}");
}

#[test]
fn catalog_and_config_reject_unknown_fields() {
    let catalog_with_version = json!({
        "locale": "en",
        "namespace": "ui",
        "entries": [],
        "version": 1
    });
    let error = serde_json::from_value::<Catalog>(catalog_with_version)
        .expect_err("unknown catalog fields must be rejected")
        .to_string();
    assert!(error.contains("unknown field `version`"), "{error}");

    let catalog_with_entry_typo = json!({
        "locale": "en",
        "namespace": "ui",
        "entries": [
            {
                "id": {"namespace": "ui", "key": "open"},
                "source": "Open",
                "template": {"text": "Open"},
                "translation": "Open"
            }
        ]
    });
    let error = serde_json::from_value::<Catalog>(catalog_with_entry_typo)
        .expect_err("unknown entry fields must be rejected")
        .to_string();
    assert!(error.contains("unknown field `translation`"), "{error}");

    let config_with_typo = json!({
        "defaultLanguage": "en",
        "fallbackLanguage": "es",
        "catalogPaths": [],
        "fallbackLangauge": "es"
    });
    let error = serde_json::from_value::<crate::i18n::I18nConfig>(config_with_typo)
        .expect_err("unknown config fields must be rejected")
        .to_string();
    assert!(
        error.contains("unknown field `fallbackLangauge`"),
        "{error}"
    );

    let set_with_schema = json!({
        "defaultLanguage": "en",
        "fallbackLanguage": "es",
        "catalogs": [],
        "schemaVersion": 1
    });
    let error = serde_json::from_value::<CatalogSet>(set_with_schema)
        .expect_err("unknown catalog-set fields must be rejected")
        .to_string();
    assert!(error.contains("unknown field `schemaVersion`"), "{error}");
}

#[test]
fn message_template_plural_categories_serde_as_lowercase_keys() {
    let template = plural(&[
        (PluralCategory::One, "{count} item"),
        (PluralCategory::Other, "{count} items"),
    ]);
    let value = serde_json::to_value(template.clone()).unwrap();
    assert_eq!(
        value,
        json!({"pluralForms": {"one": "{count} item", "other": "{count} items"}})
    );
    assert_eq!(
        serde_json::from_value::<MessageTemplate>(value).unwrap(),
        template
    );
}

#[test]
fn most_specific_requested_locale_wins_over_catalog_order() {
    let mut base = Catalog::new(locale("es"), "ui");
    base.insert("hello", entry("Hello", MessageTemplate::text("Hola base")));
    let mut regional = Catalog::new(locale("es-MX"), "ui");
    regional.insert("hello", entry("Hello", MessageTemplate::text("Hola MX")));

    let set = CatalogSet::new(locale("en"), locale("en"), vec![base, regional]);
    let resolved = set
        .resolve(&locale("es-MX"), &localized("ui", "hello", "Hello"))
        .unwrap();
    assert_eq!(resolved.display, "Hola MX");
    assert_eq!(resolved.locale, locale("es-MX"));
    assert!(!resolved.stale);
}

#[test]
fn zh_hans_cn_falls_back_to_zh_hans_before_language_only() {
    let mut language = Catalog::new(locale("zh"), "ui");
    language.insert("hello", entry("Hello", MessageTemplate::text("语言")));
    let mut script = Catalog::new(locale("zh-Hans"), "ui");
    script.insert("hello", entry("Hello", MessageTemplate::text("简体")));

    let set = CatalogSet::new(locale("en"), locale("en"), vec![language, script]);
    let resolved = set
        .resolve(&locale("zh-Hans-CN"), &localized("ui", "hello", "Hello"))
        .unwrap();
    assert_eq!(resolved.display, "简体");
    assert_eq!(resolved.locale, locale("zh-Hans"));
}

#[test]
fn fallback_is_per_message_not_per_catalog() {
    let mut regional = Catalog::new(locale("es-MX"), "ui");
    regional.insert("first", entry("First", MessageTemplate::text("Primero")));
    let mut base = Catalog::new(locale("es"), "ui");
    base.insert("second", entry("Second", MessageTemplate::text("Segundo")));

    let set = CatalogSet::new(locale("en"), locale("en"), vec![regional, base]);
    assert_eq!(
        set.resolve(&locale("es-MX"), &localized("ui", "first", "First"))
            .unwrap()
            .display,
        "Primero"
    );
    assert_eq!(
        set.resolve(&locale("es-MX"), &localized("ui", "second", "Second"))
            .unwrap()
            .display,
        "Segundo"
    );
}

#[test]
fn configured_fallback_is_used_per_message_before_source() {
    let mut fallback = Catalog::new(locale("en"), "ui");
    fallback.insert("hello", entry("Hello", MessageTemplate::text("Hello")));
    let set = CatalogSet::new(locale("es"), locale("en-US"), vec![fallback]);

    let resolved = set
        .resolve(&locale("fr-CA"), &localized("ui", "hello", "Hello"))
        .unwrap();
    assert_eq!(resolved.display, "Hello");
    assert_eq!(resolved.locale, locale("en"));
    assert!(!resolved.stale);

    let missing = set
        .resolve(
            &locale("fr-CA"),
            &localized("ui", "missing", "Source fallback"),
        )
        .unwrap();
    assert_eq!(missing.display, "Source fallback");
    assert_eq!(missing.locale, locale("en-US"));
    assert!(missing.stale);
}

#[test]
fn missing_key_returns_formatted_source_never_the_key() {
    let set = CatalogSet::new(locale("en"), locale("es"), vec![]);
    let text = localized("ui", "internal.missing.key", "Hello {name}")
        .with_arguments(arguments(&[("name", json!("Ada"))]));
    let resolved = set.resolve(&locale("fr"), &text).unwrap();
    assert_eq!(resolved.display, "Hello Ada");
    assert_eq!(resolved.source, "Hello {name}");
    assert!(resolved.stale);
    assert!(!resolved.display.contains("internal.missing.key"));
}

#[test]
fn namespaces_are_isolated() {
    let mut ui = Catalog::new(locale("en"), "ui");
    ui.insert("same", entry("Same", MessageTemplate::text("UI text")));
    let mut gameplay = Catalog::new(locale("en"), "gameplay");
    gameplay.insert(
        "same",
        entry("Same", MessageTemplate::text("Gameplay text")),
    );
    let set = CatalogSet::new(locale("en"), locale("en"), vec![ui, gameplay]);

    assert_eq!(
        set.resolve(&locale("en"), &localized("ui", "same", "Same"))
            .unwrap()
            .display,
        "UI text"
    );
    assert_eq!(
        set.resolve(&locale("en"), &localized("gameplay", "same", "Same"))
            .unwrap()
            .display,
        "Gameplay text"
    );
    assert_eq!(
        set.resolve(&locale("en"), &localized("other", "same", "Source"))
            .unwrap()
            .display,
        "Source"
    );
}

#[test]
fn catalog_context_lookup_uses_exact_key_and_context() {
    let mut catalog = Catalog::new(locale("en"), "ui");
    catalog.insert("open", entry("Open", MessageTemplate::text("Open")));
    catalog.insert_with_context(
        "open",
        "menu",
        entry("Open", MessageTemplate::text("Open menu")),
    );
    let set = CatalogSet::new(locale("en"), locale("en"), vec![catalog]);

    assert_eq!(
        set.resolve(
            &locale("en"),
            &LocalizedText::new(TextId::new("ui", "open").with_context("menu"), "Open")
        )
        .unwrap()
        .display,
        "Open menu"
    );
    assert_eq!(
        set.resolve(
            &locale("en"),
            &LocalizedText::new(
                TextId::new("ui", "open").with_context("toolbar"),
                "Open source",
            )
        )
        .unwrap()
        .display,
        "Open source"
    );
}

#[test]
fn equal_locale_catalogs_follow_declared_precedence() {
    let mut first = Catalog::new(locale("en"), "ui");
    first.insert("hello", entry("Hello", MessageTemplate::text("First")));
    let mut second = Catalog::new(locale("en"), "ui");
    second.insert("hello", entry("Hello", MessageTemplate::text("Second")));
    let set = CatalogSet::new(locale("en"), locale("en"), vec![first, second]);

    assert_eq!(
        set.resolve(&locale("en"), &localized("ui", "hello", "Hello"))
            .unwrap()
            .display,
        "First"
    );
}

#[test]
fn exact_variant_match_wins_and_variant_falls_back_to_language() {
    let mut base = Catalog::new(locale("de"), "ui");
    base.insert("hello", entry("Hello", MessageTemplate::text("Hallo")));
    let mut variant = Catalog::new(locale("de-1901"), "ui");
    variant.insert(
        "hello",
        entry("Hello", MessageTemplate::text("Hallo (1901)")),
    );
    let set = CatalogSet::new(locale("en"), locale("en"), vec![base, variant]);

    assert_eq!(
        set.resolve(&locale("de-1901"), &localized("ui", "hello", "Hello"))
            .unwrap()
            .display,
        "Hallo (1901)"
    );
    assert_eq!(
        set.resolve(&locale("de-CH"), &localized("ui", "hello", "Hello"))
            .unwrap()
            .display,
        "Hallo"
    );
}

#[test]
fn russian_cardinal_plurals_use_icu_categories() {
    let template = plural(&[
        (PluralCategory::One, "{count} файл"),
        (PluralCategory::Few, "{count} файла"),
        (PluralCategory::Many, "{count} файлов"),
        (PluralCategory::Other, "{count} файла"),
    ]);
    for (count, expected) in [
        (json!(1), "1 файл"),
        (json!(2), "2 файла"),
        (json!(5), "5 файлов"),
        (json!(21), "21 файл"),
        (json!(1.5), "1.5 файла"),
    ] {
        let resolved = crate::i18n::formatting::format_template(
            &template,
            &locale("ru"),
            &arguments(&[("count", count)]),
        )
        .unwrap();
        assert_eq!(resolved, expected);
    }
}

#[test]
fn arabic_cardinal_plurals_cover_zero_one_two_few_many_and_other() {
    let template = plural(&[
        (PluralCategory::Zero, "zero {count}"),
        (PluralCategory::One, "one {count}"),
        (PluralCategory::Two, "two {count}"),
        (PluralCategory::Few, "few {count}"),
        (PluralCategory::Many, "many {count}"),
        (PluralCategory::Other, "other {count}"),
    ]);
    for (count, expected) in [
        (json!(0), "zero 0"),
        (json!(1), "one 1"),
        (json!(2), "two 2"),
        (json!(3), "few 3"),
        (json!(11), "many 11"),
        (json!(100), "other 100"),
    ] {
        assert_eq!(
            crate::i18n::formatting::format_template(
                &template,
                &locale("ar"),
                &arguments(&[("count", count)]),
            )
            .unwrap(),
            expected
        );
    }
}

#[test]
fn decimal_visible_fraction_digits_participate_in_plural_selection() {
    let template = plural(&[
        (PluralCategory::One, "one {count}"),
        (PluralCategory::Other, "other {count}"),
    ]);
    assert_eq!(
        crate::i18n::formatting::format_template(
            &template,
            &locale("en"),
            &arguments(&[("count", json!(1))]),
        )
        .unwrap(),
        "one 1"
    );
    assert_eq!(
        crate::i18n::formatting::format_template(
            &template,
            &locale("en"),
            &arguments(&[("count", json!(1.0))]),
        )
        .unwrap(),
        "other 1.0"
    );
}

#[test]
fn named_placeholders_can_be_reordered_and_repeated() {
    let template = MessageTemplate::text("{actor} sent {item} to {recipient}; {actor} again");
    let rendered = crate::i18n::formatting::format_template(
        &template,
        &locale("en"),
        &arguments(&[
            ("actor", json!("Ada")),
            ("item", json!(3)),
            ("recipient", json!("Lin")),
        ]),
    )
    .unwrap();
    assert_eq!(rendered, "Ada sent 3 to Lin; Ada again");
}

#[test]
fn missing_placeholder_argument_has_a_clear_error() {
    let error = crate::i18n::formatting::format_template(
        &MessageTemplate::text("Hello {name}"),
        &locale("en"),
        &arguments(&[]),
    )
    .unwrap_err();
    assert_eq!(
        error,
        FormatError::MissingArgument {
            name: "name".to_owned()
        }
    );
    assert!(error.to_string().contains("{name}"));
}

#[test]
fn invalid_placeholders_are_rejected_with_positions() {
    for template in ["{}", "{0}", "{bad-name}", "{with space}"] {
        assert!(
            matches!(
                crate::i18n::formatting::format_template(
                    &MessageTemplate::text(template),
                    &locale("en"),
                    &arguments(&[]),
                ),
                Err(FormatError::InvalidPlaceholder { .. })
            ),
            "accepted {template:?}"
        );
    }
    assert!(matches!(
        crate::i18n::formatting::format_template(
            &MessageTemplate::text("before {name"),
            &locale("en"),
            &arguments(&[]),
        ),
        Err(FormatError::UnclosedPlaceholder { position: 7 })
    ));
    assert!(matches!(
        crate::i18n::formatting::format_template(
            &MessageTemplate::text("before }"),
            &locale("en"),
            &arguments(&[]),
        ),
        Err(FormatError::UnexpectedClosingBrace { position: 7 })
    ));
    assert!(matches!(
        crate::i18n::formatting::format_template(
            &MessageTemplate::text("{{name}}"),
            &locale("en"),
            &arguments(&[]),
        ),
        Err(FormatError::InvalidPlaceholder { .. })
    ));
}

#[test]
fn plural_templates_report_missing_invalid_and_unavailable_forms() {
    assert_eq!(
        crate::i18n::formatting::format_template(
            &plural(&[(PluralCategory::Other, "other")]),
            &locale("en"),
            &arguments(&[]),
        ),
        Err(FormatError::MissingPluralArgument)
    );
    assert!(matches!(
        crate::i18n::formatting::format_template(
            &plural(&[(PluralCategory::Other, "other")]),
            &locale("en"),
            &arguments(&[("count", json!(true))]),
        ),
        Err(FormatError::InvalidPluralNumber { .. })
    ));
    assert!(matches!(
        crate::i18n::formatting::format_template(
            &plural(&[(PluralCategory::One, "one")]),
            &locale("en"),
            &arguments(&[("count", json!(2))]),
        ),
        Err(FormatError::MissingPluralForm { .. })
    ));
    assert_eq!(
        crate::i18n::formatting::format_template(
            &MessageTemplate {
                text: None,
                plural_forms: None
            },
            &locale("en"),
            &arguments(&[]),
        ),
        Err(FormatError::EmptyTemplate)
    );
}

#[test]
fn plural_forms_fall_back_to_other() {
    let template = plural(&[(PluralCategory::Other, "{count} things")]);
    assert_eq!(
        crate::i18n::formatting::format_template(
            &template,
            &locale("en"),
            &arguments(&[("count", json!(2))]),
        )
        .unwrap(),
        "2 things"
    );
}

#[test]
fn lookup_and_formatting_are_separate() {
    let mut catalog = Catalog::new(locale("en"), "ui");
    catalog.insert(
        "hello",
        entry("Hello {name}", MessageTemplate::text("Hello {name}")),
    );
    let set = CatalogSet::new(locale("en"), locale("en"), vec![catalog]);
    let lookup = set.lookup(&locale("en"), &localized("ui", "hello", "Hello {name}"));

    assert_eq!(
        CatalogSet::format_lookup(&lookup, Some(&arguments(&[("name", json!("Ada"))]))).unwrap(),
        "Hello Ada"
    );
    assert_eq!(
        CatalogSet::format_lookup(&lookup, Some(&arguments(&[("name", json!("Lin"))]))).unwrap(),
        "Hello Lin"
    );
}

#[test]
fn stale_tracks_missing_or_changed_catalog_source() {
    let mut catalog = Catalog::new(locale("en"), "ui");
    catalog.insert("fresh", entry("Fresh", MessageTemplate::text("Fresco")));
    catalog.insert(
        "stale",
        entry("Old source", MessageTemplate::text("Traducción vieja")),
    );
    let set = CatalogSet::new(locale("en"), locale("en"), vec![catalog]);

    assert!(
        !set.resolve(&locale("en"), &localized("ui", "fresh", "Fresh"))
            .unwrap()
            .stale
    );
    assert!(
        set.resolve(&locale("en"), &localized("ui", "stale", "New source"))
            .unwrap()
            .stale
    );
    assert!(
        set.resolve(&locale("en"), &localized("ui", "missing", "Missing"))
            .unwrap()
            .stale
    );
}

#[test]
fn default_language_and_literal_values_are_supported() {
    let mut catalog = Catalog::new(locale("es"), "ui");
    catalog.insert("hello", entry("Hello", MessageTemplate::text("Hola")));
    let set = CatalogSet::new(locale("es-MX"), locale("en"), vec![catalog]);

    let resolved = set
        .resolve_default(&localized("ui", "hello", "Hello"))
        .unwrap();
    assert_eq!(resolved.display, "Hola");
    assert_eq!(resolved.locale, locale("es"));

    let literal = set
        .resolve_value(&locale("fr"), &TextValue::literal("Literal"))
        .unwrap();
    assert_eq!(literal.display, "Literal");
    assert_eq!(literal.locale, locale("fr"));
    assert!(!literal.stale);
}

#[test]
fn catalog_lookup_distinguishes_keys_and_contexts() {
    let mut catalog = Catalog::new(locale("en"), "ui");
    catalog.insert("a:b", entry("first", MessageTemplate::text("first")));
    catalog.insert_with_context("a", "b", entry("second", MessageTemplate::text("second")));
    assert_eq!(
        catalog.get(&TextId::new("ui", "a:b")).unwrap().source,
        "first"
    );
    assert_eq!(
        catalog
            .get(&TextId::new("ui", "a").with_context("b"))
            .unwrap()
            .source,
        "second"
    );
    assert!(catalog.get(&TextId::new("other", "a:b")).is_none());
}

#[test]
fn resolved_text_serde_uses_camel_case() {
    let set = CatalogSet::new(locale("en"), locale("en"), vec![]);
    let resolved = set
        .resolve(&locale("en"), &localized("ui", "hello", "Hello"))
        .unwrap();
    let value = serde_json::to_value(resolved).unwrap();
    assert_eq!(
        value,
        json!({
            "display": "Hello",
            "locale": "en",
            "source": "Hello",
            "stale": true
        })
    );
}

#[test]
fn catalog_json_helpers_round_trip_pretty_json_without_a_version() {
    let catalog = Catalog::from_json_str(&catalog_document_with_source(
        "en",
        "Hello {name}",
        json!({"text": "Hello {name}"}),
    ))
    .unwrap();

    let pretty = catalog.to_json_pretty().unwrap();
    assert!(pretty.contains("\n  \"locale\": \"en\""), "{pretty}");
    assert!(!pretty.contains("version"), "{pretty}");
    assert_eq!(Catalog::from_json_str(&pretty).unwrap(), catalog);
}

#[test]
fn catalog_loading_reads_paths_from_disk_in_declared_order() {
    let directory = TestCatalogDirectory::new("load");
    let english = directory.write("en.json", &catalog_document(json!({"text": "Hello"})));
    let spanish_json = catalog_document_with_source("es", "Hello", json!({"text": "Hola"}));
    let spanish = directory.write("es.json", &spanish_json);

    let loaded = CatalogSet::load(&[spanish.clone(), english.clone()]).unwrap();
    assert_eq!(loaded.default_language, locale("en"));
    assert_eq!(loaded.fallback_language, locale("en"));
    assert_eq!(
        loaded
            .catalogs
            .iter()
            .map(|catalog| catalog.locale.clone())
            .collect::<Vec<_>>(),
        vec![locale("es"), locale("en")]
    );

    let config = I18nConfig {
        default_language: locale("es"),
        fallback_language: locale("en"),
        catalog_paths: vec![english, spanish],
    };
    let configured = config.load_catalogs().unwrap();
    assert_eq!(configured.default_language, locale("es"));
    assert_eq!(configured.fallback_language, locale("en"));
    assert_eq!(
        configured
            .catalogs
            .iter()
            .map(|catalog| catalog.locale.clone())
            .collect::<Vec<_>>(),
        vec![locale("en"), locale("es")]
    );
}

#[test]
fn catalog_loading_reports_path_and_json_errors() {
    let directory = TestCatalogDirectory::new("errors");
    let missing = directory.path.join("missing.json");
    let error = CatalogSet::load(&[missing]).unwrap_err();
    assert!(error.contains("failed to read catalog"), "{error}");
    assert!(error.contains("missing.json"), "{error}");

    let invalid = directory.write("invalid.json", "{");
    let config = I18nConfig {
        catalog_paths: vec![invalid],
        ..I18nConfig::default()
    };
    let error = config.load_catalogs().unwrap_err();
    assert!(error.contains("failed to parse catalog"), "{error}");
    assert!(error.contains("invalid.json"), "{error}");
}

#[test]
fn catalog_loading_rejects_invalid_placeholders() {
    let directory = TestCatalogDirectory::new("invalid-placeholder");
    let path = directory.write(
        "invalid-placeholder.json",
        &catalog_document(json!({"text": "Hello {bad-name}"})),
    );

    let error = CatalogSet::load(&[path]).unwrap_err();
    assert!(error.contains("invalid placeholder"), "{error}");
    assert!(error.contains("bad-name"), "{error}");
}

#[test]
fn catalog_loading_rejects_empty_templates_and_plural_forms_without_other() {
    for (template, expected) in [
        (json!({}), "must define either text or plural forms"),
        (json!({"text": "   "}), "template text must not be empty"),
        (json!({"pluralForms": {}}), "plural forms must not be empty"),
        (
            json!({"pluralForms": {"one": "{count} item"}}),
            "must include the 'other' category",
        ),
        (
            json!({"pluralForms": {"other": ""}}),
            "template text must not be empty",
        ),
        (
            json!({"text": "Hello", "pluralForms": {"other": "Hello"}}),
            "must not define both text and plural forms",
        ),
    ] {
        let error = Catalog::from_json_str(&catalog_document(template))
            .expect_err("invalid template must be rejected")
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn catalog_validation_rejects_missing_placeholders() {
    let error = Catalog::from_json_str(&catalog_document_with_source(
        "en",
        "Hello {name}",
        json!({"text": "Hola"}),
    ))
    .expect_err("missing placeholders must be rejected")
    .to_string();

    assert!(error.contains("placeholder mismatch in text"), "{error}");
    assert!(error.contains("missing placeholders"), "{error}");
    assert!(error.contains("\"name\""), "{error}");
}

#[test]
fn catalog_validation_rejects_extra_placeholders() {
    let error = Catalog::from_json_str(&catalog_document_with_source(
        "en",
        "Hello",
        json!({"text": "Hola {name}"}),
    ))
    .expect_err("extra placeholders must be rejected")
    .to_string();

    assert!(error.contains("placeholder mismatch in text"), "{error}");
    assert!(error.contains("extra placeholders"), "{error}");
    assert!(error.contains("\"name\""), "{error}");
}

#[test]
fn catalog_validation_allows_reordered_placeholders() {
    let catalog = Catalog::from_json_str(&catalog_document_with_source(
        "en",
        "Hello {name}, {count}",
        json!({"text": "{count}: Hola {name}"}),
    ))
    .expect("reordering the same placeholders should be accepted");

    assert_eq!(
        catalog.entries.values().next().unwrap().template,
        MessageTemplate::text("{count}: Hola {name}")
    );
}

#[test]
fn catalog_validation_compares_placeholders_in_every_plural_form() {
    let error = Catalog::from_json_str(&catalog_document_with_source(
        "en",
        "{count} item",
        json!({
            "pluralForms": {
                "one": "{count} item",
                "other": "{name} items"
            }
        }),
    ))
    .expect_err("each plural form must preserve the source placeholders")
    .to_string();

    assert!(
        error.contains("placeholder mismatch in other plural form"),
        "{error}"
    );
    assert!(error.contains("missing placeholders"), "{error}");
    assert!(error.contains("\"count\""), "{error}");
    assert!(error.contains("extra placeholders"), "{error}");
    assert!(error.contains("\"name\""), "{error}");

    let catalog = Catalog::from_json_str(&catalog_document_with_source(
        "en",
        "{count} item",
        json!({
            "pluralForms": {
                "one": "item {count}",
                "other": "{count} items"
            }
        }),
    ))
    .expect("plural forms may reorder the source placeholders");
    assert_eq!(
        catalog
            .entries
            .values()
            .next()
            .unwrap()
            .template
            .plural_forms
            .as_ref()
            .unwrap()[&PluralCategory::One],
        "item {count}"
    );
}

#[test]
fn pseudolocalization_preserves_placeholders_verbatim() {
    let config = PseudolocalizationConfig {
        expansion_ratio: 1.0,
        prefix: "<".to_owned(),
        suffix: ">".to_owned(),
        replace_accents: false,
        fake_bidi: false,
    };
    let template = "{name} has {item_count} and {name}";

    let pseudolocalized = pseudolocalize_template(template, &config).unwrap();
    assert_eq!(pseudolocalized, format!("<{template}>"));
    assert_eq!(
        config.pseudolocalize_template(template).unwrap(),
        pseudolocalized
    );
}

#[test]
fn pseudolocalization_expands_literal_text_without_changing_placeholders() {
    let config = PseudolocalizationConfig {
        expansion_ratio: 2.0,
        prefix: String::new(),
        suffix: String::new(),
        replace_accents: false,
        fake_bidi: false,
    };

    let pseudolocalized = pseudolocalize_template("Hi{name}", &config).unwrap();
    assert_eq!(pseudolocalized, "Hi{name}~~");
    assert_eq!(pseudolocalized.matches("{name}").count(), 1);
}

#[test]
fn pseudolocalization_caps_expansion_ratio_and_additional_characters() {
    let config = PseudolocalizationConfig {
        expansion_ratio: f32::MAX,
        prefix: String::new(),
        suffix: String::new(),
        replace_accents: false,
        fake_bidi: false,
    };

    let ten_times_source = "x".repeat(100);
    let ten_times_expanded = pseudolocalize_template(&ten_times_source, &config).unwrap();
    assert_eq!(ten_times_expanded.chars().count(), 1_000);
    assert_eq!(ten_times_expanded.matches('~').count(), 900);

    let additional_cap_source = "x".repeat(1_000);
    let additional_cap_expanded = pseudolocalize_template(&additional_cap_source, &config).unwrap();
    assert_eq!(additional_cap_expanded.chars().count(), 1_000 + 4_096);
    assert_eq!(additional_cap_expanded.matches('~').count(), 4_096);
}

#[test]
fn pseudolocalization_optionally_adds_accents_and_rtl_marks() {
    let config = PseudolocalizationConfig {
        expansion_ratio: 1.0,
        prefix: "[".to_owned(),
        suffix: "]".to_owned(),
        replace_accents: true,
        fake_bidi: true,
    };

    assert_eq!(
        pseudolocalize_template("Hola {name}", &config).unwrap(),
        "\u{200f}[Hólá {name}]\u{200f}"
    );
}

#[test]
fn pseudolocalization_rejects_invalid_placeholders() {
    let config = PseudolocalizationConfig::default();
    assert!(matches!(
        pseudolocalize_template("Hello {", &config),
        Err(FormatError::UnclosedPlaceholder { position: 6 })
    ));
}
