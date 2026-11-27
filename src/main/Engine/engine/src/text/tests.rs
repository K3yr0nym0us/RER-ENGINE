use std::collections::BTreeSet;

use super::{
    FontFamily, FontSource, FontWeight, RasterizedText, ShapedGlyph, TextAlign, TextStyle,
    TextSystem, TextWrap,
};

fn test_style(family: FontFamily) -> TextStyle {
    TextStyle {
        max_width: Some(320.0),
        max_height: Some(120.0),
        ..TextStyle::new(family, 24.0)
    }
}

fn has_ink(raster: &RasterizedText) -> bool {
    raster
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .any(|pixel| pixel[3] > 0)
}

#[test]
fn hud_and_prompt_styles_use_the_contract_sizes_and_shaping() {
    let hud = super::hud_style(FontFamily::SansSerif, 320, 100, [1.0; 4], Some("es"));
    assert_eq!(hud.size, 62.0);
    assert_eq!(hud.max_width, Some(320.0));
    assert_eq!(hud.max_height, Some(100.0));
    assert_eq!(hud.align, TextAlign::Center);
    assert_eq!(hud.wrap, TextWrap::WordOrGlyph);
    assert_eq!(hud.supersample, 1);

    let prompt = super::prompt_style(
        FontFamily::SansSerif,
        320,
        100,
        FontWeight::Bold,
        Some("ar"),
    );
    assert_eq!(prompt.size, 78.0);
    assert_eq!(prompt.weight, FontWeight::Bold);
    assert_eq!(prompt.align, TextAlign::Center);
    assert_eq!(prompt.wrap, TextWrap::WordOrGlyph);
    assert_eq!(prompt.supersample, 4);
}

#[test]
fn font_validation_accepts_embedded_fonts_and_rejects_invalid_data() {
    assert!(TextSystem::validate_font_bytes(FontSource::InterRegular.bytes()).is_ok());
    assert!(TextSystem::validate_font_bytes(b"not a font").is_err());
    assert!(super::validate_font_file("font.png").is_err());
}

fn ink_bounds(raster: &RasterizedText) -> (u32, u32, u32, u32) {
    let mut min_x = raster.width;
    let mut max_x = 0;
    let mut min_y = raster.height;
    let mut max_y = 0;
    for (index, pixel) in raster.rgba.as_chunks::<4>().0.iter().enumerate() {
        if pixel[3] == 0 {
            continue;
        }
        let x = (index as u32) % raster.width;
        let y = (index as u32) / raster.width;
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    (min_x, min_y, max_x, max_y)
}

fn glyph_starting_at(raster: &RasterizedText, offset: usize) -> &ShapedGlyph {
    raster
        .glyphs
        .iter()
        .find(|glyph| glyph.start == offset)
        .expect("glyph at UTF-8 offset")
}

#[test]
fn ascii_and_accents_rasterize_with_utf8_clusters() {
    let mut text = TextSystem::new();
    let style = TextStyle {
        tint: [0.2, 0.4, 0.6, 1.0],
        ..test_style(FontFamily::SansSerif)
    };
    let raster = text.rasterize("Héllo, RER!", &style);

    assert!(has_ink(&raster));
    assert_eq!(
        raster.rgba.len(),
        (raster.width * raster.height * 4) as usize
    );
    let accent = glyph_starting_at(&raster, 1);
    assert_eq!((accent.start, accent.end), (1, 3));
    assert!(
        raster
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .all(|pixel| pixel[..3] == [51, 102, 153])
    );
}

#[test]
fn arabic_uses_contextual_forms_and_rtl_levels() {
    let mut text = TextSystem::new();
    let style = test_style(FontFamily::Arabic);
    let word = text.rasterize("سلام", &style);
    assert!(has_ink(&word));
    assert!(!word.glyphs.is_empty());
    assert!(word.glyphs.iter().all(|glyph| glyph.level % 2 == 1));

    let isolated: BTreeSet<_> = ['س', 'ل', 'ا', 'م']
        .into_iter()
        .flat_map(|character| text.rasterize(&character.to_string(), &style).glyphs)
        .map(|glyph| glyph.glyph_id)
        .collect();
    assert!(
        word.glyphs
            .iter()
            .any(|glyph| !isolated.contains(&glyph.glyph_id))
    );
}

#[test]
fn mixed_hebrew_uses_bidi_levels_and_font_fallback() {
    let mut text = TextSystem::new();
    let value = "אב abc גד";
    let raster = text.rasterize(value, &test_style(FontFamily::Hebrew));
    let hebrew = glyph_starting_at(&raster, 0);
    let latin = glyph_starting_at(&raster, value.find('a').expect("latin offset"));

    assert_eq!(hebrew.level % 2, 1);
    assert_eq!(latin.level % 2, 0);
    let noto = text
        .loaded_fonts()
        .iter()
        .find(|font| font.source == FontSource::NotoHebrew)
        .expect("Noto Hebrew loaded");
    assert_eq!(hebrew.font_id, noto.id);
    assert!(has_ink(&raster));
}

#[test]
fn cjk_falls_back_to_a_distinct_embedded_font() {
    let mut text = TextSystem::new();
    let raster = text.rasterize("A界B", &test_style(FontFamily::SansSerif));
    let latin = glyph_starting_at(&raster, 0);
    let han = glyph_starting_at(&raster, "A".len());

    assert_ne!(latin.font_id, han.font_id);
    let noto = text
        .loaded_fonts()
        .iter()
        .find(|font| font.source == FontSource::NotoSansSC)
        .expect("Noto SC loaded");
    assert_eq!(han.font_id, noto.id);
}

#[test]
fn fira_mono_shapes_fi_and_fl_ligatures() {
    let mut text = TextSystem::new();
    let raster = text.rasterize("fi fl", &test_style(FontFamily::Monospace));
    let fira = text
        .loaded_fonts()
        .iter()
        .find(|font| font.source == FontSource::FiraMono)
        .expect("Fira Mono loaded");

    assert!(raster.glyphs.iter().all(|glyph| glyph.font_id == fira.id));
    assert!(
        raster
            .glyphs
            .iter()
            .any(|glyph| glyph.end - glyph.start == 2)
    );
    assert!(raster.glyphs.len() < "fi fl".chars().count());
}

#[test]
fn caret_accepts_character_indices_and_handles_multibyte_utf8() {
    let mut text = TextSystem::new();
    let value = "aé界z";
    let style = test_style(FontFamily::SansSerif);
    let raster = text.rasterize(value, &style);
    assert!(
        raster
            .glyphs
            .iter()
            .all(|glyph| value.is_char_boundary(glyph.start) && value.is_char_boundary(glyph.end))
    );

    let positions: Vec<_> = (0..=value.chars().count())
        .map(|caret| text.caret_x(value, caret, &style).expect("caret position"))
        .collect();
    assert!(positions.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(positions[2] > positions[1]);
    assert!(positions[2] < positions[3]);
}

#[test]
fn cjk_wraps_at_glyph_boundaries() {
    let mut text = TextSystem::new();
    let style = TextStyle {
        wrap: TextWrap::Glyph,
        max_width: Some(52.0),
        max_height: Some(150.0),
        ..TextStyle::new(FontFamily::SimplifiedChinese, 24.0)
    };
    let raster = text.rasterize("中文字符测试", &style);
    let rows: BTreeSet<_> = raster
        .glyphs
        .iter()
        .map(|glyph| glyph.y.round() as i32)
        .collect();

    assert!(rows.len() > 1, "expected multiple CJK rows, got {rows:?}");
    assert_eq!(raster.width, 52);
    assert!(has_ink(&raster));
}

#[test]
fn custom_project_font_is_primary_and_keeps_builtin_fallback() {
    let mut text = TextSystem::new();
    let custom_id = text
        .font_from_bytes("Project Mono", FontSource::FiraMono.bytes().to_vec())
        .expect("register custom font bytes");
    let style = test_style(FontFamily::Custom("Project Mono".to_owned()));
    let raster = text.rasterize("A界B", &style);
    let latin = glyph_starting_at(&raster, 0);
    let han = glyph_starting_at(&raster, "A".len());
    let noto = text
        .loaded_fonts()
        .iter()
        .find(|font| font.source == FontSource::NotoSansSC)
        .expect("Noto SC loaded");

    assert_eq!(latin.font_id, custom_id);
    assert_eq!(han.font_id, noto.id);
    assert_ne!(han.font_id, custom_id);
    assert!(has_ink(&raster));

    text.set_locale("ar");
    let after_locale = text.rasterize("A", &style);
    assert_eq!(glyph_starting_at(&after_locale, 0).font_id, custom_id);
}

#[test]
fn custom_font_aliases_reject_reserved_builtin_names() {
    let mut text = TextSystem::new();
    for alias in [
        "sans_serif",
        "arabic",
        "hebrew",
        "simplified_chinese",
        "monospace",
    ] {
        let error = text
            .font_from_bytes(alias, FontSource::FiraMono.bytes().to_vec())
            .expect_err("reserved custom alias");
        assert!(error.contains("reserved"), "{error}");
    }
    assert_eq!(text.loaded_fonts().len(), 6);
}

#[test]
fn load_font_registers_a_project_file() {
    let mut text = TextSystem::new();
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../assets/fonts/FiraMono-Medium.ttf");
    let id = text
        .load_font("File Mono", path)
        .expect("register project font file");
    let style = test_style(FontFamily::Custom("File Mono".to_owned()));
    let raster = text.rasterize("file", &style);

    assert!(raster.glyphs.iter().all(|glyph| glyph.font_id == id));
    assert!(has_ink(&raster));
}

#[test]
fn rasterized_text_reports_effective_supersampling() {
    let mut text = TextSystem::new();
    let style = TextStyle {
        supersample: 2,
        ..test_style(FontFamily::SansSerif)
    };
    let raster = text.rasterize("Hi", &style);

    assert_eq!(raster.supersample, 2);
    assert_eq!(
        raster.rgba.len(),
        (raster.width * raster.height * 4) as usize
    );
    assert!(raster.glyphs.iter().all(|glyph| {
        glyph.cache_key.font_size_bits == (style.size * raster.supersample as f32).to_bits()
    }));
}

#[test]
fn empty_text_is_transparent_and_centers_the_caret() {
    let mut text = TextSystem::new();
    let style = TextStyle {
        max_width: Some(120.0),
        max_height: Some(40.0),
        ..TextStyle::new(FontFamily::SansSerif, 24.0)
    };
    let raster = text.rasterize("", &style);

    assert_eq!((raster.width, raster.height), (120, 40));
    assert_eq!(raster.supersample, 1);
    assert!(raster.glyphs.is_empty());
    assert!(raster.rgba.iter().all(|byte| *byte == 0));
    assert_eq!(text.caret_x("", 0, &style), Some(60.0));
}

#[test]
fn caret_positions_follow_rtl_visual_order() {
    let mut text = TextSystem::new();
    let value = "سلام";
    let style = test_style(FontFamily::Arabic);
    let positions: Vec<_> = (0..=value.chars().count())
        .map(|caret| text.caret_x(value, caret, &style).expect("RTL caret"))
        .collect();

    assert!(positions.windows(2).all(|pair| pair[0] > pair[1]));
}

#[test]
fn caret_positions_select_the_matching_mixed_bidi_run() {
    let mut text = TextSystem::new();
    let value = "אב abc גד";
    let style = test_style(FontFamily::Hebrew);
    let positions: Vec<_> = (0..=value.chars().count())
        .map(|caret| text.caret_x(value, caret, &style).expect("mixed caret"))
        .collect();

    assert!(positions[0] > positions[1]);
    assert!(positions[1] > positions[2]);
    assert!(positions[3] < positions[4]);
    assert!(positions[4] < positions[5]);
    assert!(positions[6].is_finite());
    assert!(positions[7] > positions[8]);
}

#[test]
fn invalid_style_language_warns_and_keeps_the_current_locale() {
    let mut text = TextSystem::new();
    text.set_locale("ar");
    let style = TextStyle {
        language: Some("not a locale!".to_owned()),
        ..test_style(FontFamily::Arabic)
    };
    let raster = text.rasterize("سلام", &style);

    assert!(has_ink(&raster));
    assert_eq!(text.locale(), "ar");
}

#[test]
fn text_style_serde_round_trip_preserves_every_field() {
    let style = TextStyle {
        family: FontFamily::Arabic,
        size: 21.5,
        line_height: 29.0,
        weight: FontWeight::Bold,
        wrap: TextWrap::Glyph,
        align: TextAlign::Right,
        language: Some("ar-EG".to_owned()),
        max_width: Some(240.0),
        max_height: Some(96.0),
        tint: [0.1, 0.2, 0.3, 0.4],
        supersample: 3,
    };
    let json = serde_json::to_string(&style).expect("serialize TextStyle");
    let decoded: TextStyle = serde_json::from_str(&json).expect("deserialize TextStyle");

    assert_eq!(decoded, style);
    assert_eq!(
        json,
        r#"{"family":"arabic","size":21.5,"lineHeight":29.0,"weight":"bold","wrap":"glyph","align":"right","language":"ar-EG","maxWidth":240.0,"maxHeight":96.0,"tint":[0.1,0.2,0.3,0.4],"supersample":3}"#
    );
}

#[test]
fn custom_font_family_serializes_as_a_plain_string() {
    let family = FontFamily::Custom("Project Mono".to_owned());
    let json = serde_json::to_string(&family).expect("serialize custom family");
    let decoded: FontFamily = serde_json::from_str(&json).expect("deserialize custom family");

    assert_eq!(json, r#""Project Mono""#);
    assert_eq!(decoded, family);
    assert_eq!(
        serde_json::to_string(&FontFamily::SansSerif).expect("serialize built-in family"),
        r#""sans_serif""#
    );
}

#[test]
fn horizontal_alignment_controls_ink_bounds_and_caret() {
    assert_eq!(TextAlign::default(), TextAlign::Center);
    assert_eq!(TextStyle::default().align, TextAlign::Center);

    let mut text = TextSystem::new();
    let value = "Héllo";
    let mut left_edge = None;
    for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
        let style = TextStyle {
            align,
            max_width: Some(128.0),
            max_height: Some(64.0),
            ..TextStyle::new(FontFamily::SansSerif, 24.0)
        };
        let raster = text.rasterize(value, &style);
        let (min_x, min_y, max_x, max_y) = ink_bounds(&raster);
        let caret_start = text.caret_x(value, 0, &style).expect("start caret");
        let caret_end = text
            .caret_x(value, value.chars().count(), &style)
            .expect("end caret");

        match align {
            TextAlign::Left => {
                assert!(min_x <= 1, "left ink starts at {min_x}");
                left_edge = Some(min_x);
            }
            TextAlign::Center => {
                assert!(((min_x + max_x + 1) as f32 / 2.0 - 64.0).abs() <= 2.0);
            }
            TextAlign::Right => {
                assert!(max_x >= 126, "right ink ends at {max_x}");
                if let Some(left) = left_edge {
                    assert!(left < min_x);
                }
            }
        }
        assert!(((min_y + max_y + 1) as f32 / 2.0 - 32.0).abs() <= 2.0);
        assert!((caret_start - min_x as f32).abs() <= 4.0);
        assert!((caret_end - (max_x + 1) as f32).abs() <= 4.0);
    }

    for (align, expected) in [
        (TextAlign::Left, 0.0),
        (TextAlign::Center, 64.0),
        (TextAlign::Right, 128.0),
    ] {
        let style = TextStyle {
            align,
            max_width: Some(128.0),
            max_height: Some(64.0),
            ..TextStyle::new(FontFamily::SansSerif, 24.0)
        };
        assert_eq!(text.caret_x("", 0, &style), Some(expected));
    }
}

#[test]
fn ink_is_centered_and_clipped_to_the_requested_rect() {
    let mut text = TextSystem::new();
    let style = TextStyle {
        max_width: Some(128.0),
        max_height: Some(64.0),
        ..TextStyle::new(FontFamily::SansSerif, 24.0)
    };
    let raster = text.rasterize("Héllo", &style);
    let mut min_x = raster.width;
    let mut max_x = 0;
    let mut min_y = raster.height;
    let mut max_y = 0;
    for (index, pixel) in raster.rgba.as_chunks::<4>().0.iter().enumerate() {
        if pixel[3] == 0 {
            continue;
        }
        let x = (index as u32) % raster.width;
        let y = (index as u32) / raster.width;
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }

    assert_eq!((raster.width, raster.height), (128, 64));
    assert!(has_ink(&raster));
    assert!(((min_x + max_x + 1) as f32 / 2.0 - 64.0).abs() <= 2.0);
    assert!(((min_y + max_y + 1) as f32 / 2.0 - 32.0).abs() <= 2.0);
}

#[test]
fn set_locale_normalizes_and_rejects_invalid_values() {
    let mut text = TextSystem::new();
    text.set_locale(" AR_eg ");
    assert_eq!(text.locale(), "ar-EG");

    text.set_locale("not a locale!");
    assert_eq!(text.locale(), "ar-EG");
    text.set_locale("   ");
    assert_eq!(text.locale(), "ar-EG");
}

#[test]
fn locale_change_preserves_embedded_font_ids() {
    let mut text = TextSystem::new();
    let before: Vec<_> = text.loaded_fonts().iter().map(|font| font.id).collect();
    assert_eq!(text.loaded_fonts().len(), 6);
    assert_eq!(before.iter().collect::<BTreeSet<_>>().len(), 6);

    text.set_locale("ar");
    assert_eq!(text.locale(), "ar");
    let after: Vec<_> = text.loaded_fonts().iter().map(|font| font.id).collect();
    assert_eq!(after, before);
}
