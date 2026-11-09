use super::*;

pub(super) fn camera_body(
    ui: &mut egui::Ui,
    state: &mut SidebarLeftState,
    context: ProjectContext,
    camera: Option<Camera2d>,
    play_view: Option<PlayView>,
    ready: bool,
    locale: Locale,
    hint: &str,
    actions: &mut Vec<LeftAction>,
) {
    if context.is_3d() {
        widgets::caption(
            ui,
            crate::editor::i18n::tr(locale, "Tipo de cámara", "Camera type"),
        );
        widgets::select_placeholder(ui, context.game_style.as_str(), hint);
        ui.add_space(4.0);

        if context.game_style != GameStyle::FirstPerson {
            widgets::note(
                ui,
                crate::editor::i18n::tr(
                    locale,
                    "Los controles detallados para este modo aún no están disponibles.",
                    "Detailed camera controls for this mode are not available yet.",
                ),
            );
            return;
        }

        let Some(view) = play_view else {
            widgets::note(
                ui,
                crate::editor::i18n::tr(
                    locale,
                    "Aún no hay jugador en la escena.",
                    "No player in the scene yet.",
                ),
            );
            return;
        };
        if view.player_id.is_none() {
            widgets::note(
                ui,
                crate::editor::i18n::tr(
                    locale,
                    "Aún no hay jugador en la escena.",
                    "No player in the scene yet.",
                ),
            );
            return;
        }

        state.cam3_x.sync(view.eye[0]);
        state.cam3_y.sync(view.eye[1]);
        state.cam3_z.sync(view.eye[2]);
        state.cam3_yaw.sync(view.yaw.to_degrees());
        state.cam3_fov.sync(view.fov_y.to_degrees());
        state.cam3_frustum.sync(view.frustum_distance);

        widgets::caption(ui, crate::editor::i18n::tr(locale, "Cámara", "Camera"));
        let mut axis = None;
        ui.add_enabled_ui(ready, |ui| {
            ui.columns(3, |columns| {
                for (index, draft) in [
                    (0usize, &mut state.cam3_x),
                    (1, &mut state.cam3_y),
                    (2, &mut state.cam3_z),
                ] {
                    let width = columns[index].available_width();
                    columns[index].label(
                        RichText::new(["X", "Y", "Z"][index])
                            .size(11.5)
                            .color(crate::editor::theme::TEXT_MUTED),
                    );
                    if widgets::number_input(&mut columns[index], width, &mut draft.text)
                        && let Some(value) = draft.parsed()
                    {
                        axis = Some((index as u8, value));
                    }
                }
            });
        });
        if let Some((axis, value)) = axis {
            actions.push(LeftAction::SetPlayCameraAxis { axis, value });
        }

        let mut yaw = None;
        let mut fov = None;
        let mut frustum = None;
        ui.add_enabled_ui(ready, |ui| {
            ui.columns(3, |columns| {
                let labels = [
                    crate::editor::i18n::tr(locale, "Yaw (°)", "Yaw (°)"),
                    crate::editor::i18n::tr(locale, "FOV (°)", "FOV (°)"),
                    crate::editor::i18n::tr(locale, "Rango F. (m)", "F. Range (m)"),
                ];
                let drafts = [
                    &mut state.cam3_yaw,
                    &mut state.cam3_fov,
                    &mut state.cam3_frustum,
                ];
                for (index, (label, draft)) in labels.into_iter().zip(drafts).enumerate() {
                    let width = columns[index].available_width();
                    columns[index].label(
                        RichText::new(label)
                            .size(11.5)
                            .color(crate::editor::theme::TEXT_MUTED),
                    );
                    if widgets::number_input(&mut columns[index], width, &mut draft.text)
                        && let Some(value) = draft.parsed()
                    {
                        match index {
                            0 => yaw = Some(value.to_radians()),
                            1 => fov = Some(value.to_radians()),
                            _ => frustum = Some(value.max(0.5)),
                        }
                    }
                }
            });
        });
        if let Some(value) = yaw {
            actions.push(LeftAction::SetPlayCameraYaw(value));
        }
        if let Some(value) = fov {
            actions.push(LeftAction::SetPlayCameraFov(value));
        }
        if let Some(value) = frustum {
            actions.push(LeftAction::SetPlayCameraFrustum(value));
        }
        return;
    }

    let camera = camera.unwrap_or(Camera2d {
        x: 0.0,
        y: 0.0,
        half_h: 10.0,
    });
    state.cam_x.sync(camera.x);
    state.cam_y.sync(camera.y);
    state.cam_zoom.sync(camera.half_h);

    widgets::caption(
        ui,
        crate::editor::i18n::tr(locale, "Cámara del editor", "Editor camera"),
    );
    let mut commit = false;
    ui.columns(3, |columns| {
        let width = columns[0].available_width();
        columns[0].label(
            RichText::new("X")
                .size(11.5)
                .color(crate::editor::theme::TEXT_MUTED),
        );
        if widgets::number_input(&mut columns[0], width, &mut state.cam_x.text) {
            commit = true;
        }

        let width = columns[1].available_width();
        columns[1].label(
            RichText::new("Y")
                .size(11.5)
                .color(crate::editor::theme::TEXT_MUTED),
        );
        if widgets::number_input(&mut columns[1], width, &mut state.cam_y.text) {
            commit = true;
        }

        let width = columns[2].available_width();
        columns[2].label(
            RichText::new(crate::editor::i18n::tr(locale, "Zoom", "Zoom"))
                .size(11.5)
                .color(crate::editor::theme::TEXT_MUTED),
        );
        if widgets::number_input(&mut columns[2], width, &mut state.cam_zoom.text) {
            commit = true;
        }
    });

    if commit
        && let (Some(x), Some(y), Some(half_h)) = (
            state.cam_x.parsed(),
            state.cam_y.parsed(),
            state.cam_zoom.parsed(),
        )
    {
        actions.push(LeftAction::SetCamera2d {
            x,
            y,
            half_h: half_h.max(1.0),
        });
    }
}
