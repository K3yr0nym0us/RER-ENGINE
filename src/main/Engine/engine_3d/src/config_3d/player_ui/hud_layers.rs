//! Orden de dibujo y hit-test HUD por `z_index` (textos, botones, imágenes y objetos).

use super::config::{PlayerUiButton, PlayerUiImage, PlayerUiObject, PlayerUiTextBox};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HudLayerKind {
    Text,
    Button,
    Image,
    Object,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct HudLayerRef {
    pub kind: HudLayerKind,
    pub index: usize,
}

/// Calcula el siguiente `z_index` libre en la pantalla.
pub(crate) fn next_z_index_for_screen(
    texts: Option<&[PlayerUiTextBox]>,
    buttons: Option<&[PlayerUiButton]>,
    images: Option<&[PlayerUiImage]>,
    objects: Option<&[PlayerUiObject]>,
) -> i32 {
    let mut max_z = 0i32;
    if let Some(list) = texts {
        max_z = max_z.max(list.iter().map(|b| b.z_index).max().unwrap_or(0));
    }
    if let Some(list) = buttons {
        max_z = max_z.max(list.iter().map(|b| b.z_index).max().unwrap_or(0));
    }
    if let Some(list) = images {
        max_z = max_z.max(list.iter().map(|i| i.z_index).max().unwrap_or(0));
    }
    if let Some(list) = objects {
        max_z = max_z.max(list.iter().map(|o| o.z_index).max().unwrap_or(0));
    }
    max_z.saturating_add(1)
}

/// Orden ascendente para dibujar (menor z_index primero).
pub(crate) fn hud_draw_order(
    texts: &[PlayerUiTextBox],
    buttons: &[PlayerUiButton],
    images: &[PlayerUiImage],
    objects: &[PlayerUiObject],
) -> Vec<HudLayerRef> {
    let mut layers = Vec::new();
    for (i, b) in texts.iter().enumerate() {
        layers.push((b.z_index, b.id, HudLayerKind::Text, i));
    }
    for (i, b) in buttons.iter().enumerate() {
        layers.push((b.z_index, b.id, HudLayerKind::Button, i));
    }
    for (i, img) in images.iter().enumerate() {
        layers.push((img.z_index, img.id, HudLayerKind::Image, i));
    }
    for (i, obj) in objects.iter().enumerate() {
        layers.push((obj.z_index, obj.id, HudLayerKind::Object, i));
    }
    layers.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    layers
        .into_iter()
        .map(|(_, _, kind, index)| HudLayerRef { kind, index })
        .collect()
}

/// Orden descendente para hit-test (mayor z_index encima).
pub(crate) fn hud_hit_test_order(
    texts: &[PlayerUiTextBox],
    buttons: &[PlayerUiButton],
    images: &[PlayerUiImage],
    objects: &[PlayerUiObject],
) -> Vec<HudLayerRef> {
    let mut order = hud_draw_order(texts, buttons, images, objects);
    order.reverse();
    order
}
