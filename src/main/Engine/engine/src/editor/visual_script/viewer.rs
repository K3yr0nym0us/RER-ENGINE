//! Viewer de `egui-snarl` para los nodos de lógica de RER.
//!
//! Reproduce el aspecto del `VisualScriptNode` de React: cabecera coloreada
//! según categoría (cian/verde/azul), cuerpo con el resumen de datos, pines
//! circulares grises con variantes verde/rojo/ámbar y fondo de rejilla.

use egui::{Color32, Frame, Margin, RichText, Stroke, Ui, Vec2};
use egui_snarl::{
    InPin, NodeId, OutPin, Snarl,
    ui::{
        BackgroundPattern, Grid, NodeLayout, PinInfo, PinPlacement, SnarlStyle, SnarlViewer,
        WireStyle,
    },
};

use crate::editor::cli::Locale;
use crate::editor::theme;

use super::{
    GraphContext, NodeCategory, NodeKind, Pin, VisualNode, chrono_stamp, node_summary, pin_layout,
};

const REACT_BG: Color32 = Color32::from_rgb(0x1e, 0x28, 0x38);
const REACT_GRID: Color32 = Color32::from_rgb(0x3d, 0x4d, 0x66);
const REACT_NODE_BG: Color32 = Color32::from_rgb(0x21, 0x25, 0x29);
const REACT_TEXT: Color32 = Color32::from_rgb(0xe8, 0xec, 0xf4);

pub(crate) fn category_color(category: NodeCategory) -> Color32 {
    match category {
        NodeCategory::Event => theme::INFO,
        NodeCategory::Flow => theme::CONTROL_BLUE,
        NodeCategory::Action => theme::SUCCESS,
    }
}

/// Estilo del lienzo, alineado con el CSS `.visual-scripting-modal`.
pub(crate) fn snarl_style() -> SnarlStyle {
    SnarlStyle {
        node_layout: Some(NodeLayout::coil()),
        pin_placement: Some(PinPlacement::Edge),
        pin_size: Some(10.0),
        pin_stroke: Some(Stroke::new(1.0, REACT_GRID)),
        wire_width: Some(2.5),
        wire_frame_size: Some(48.0),
        wire_style: Some(WireStyle::Bezier5),
        collapsible: Some(false),
        header_drag_space: Some(Vec2::new(150.0, 0.0)),
        bg_pattern: Some(BackgroundPattern::Grid(Grid::new(
            Vec2::new(16.0, 16.0),
            0.0,
        ))),
        bg_pattern_stroke: Some(Stroke::new(1.0, REACT_GRID)),
        bg_frame: Some(
            Frame::new()
                .inner_margin(Margin::ZERO)
                .fill(REACT_BG)
                .stroke(Stroke::NONE),
        ),
        node_frame: Some(
            Frame::new()
                .inner_margin(Margin {
                    left: 8,
                    right: 8,
                    top: 6,
                    bottom: 6,
                })
                .corner_radius(egui::CornerRadius::same(5))
                .fill(REACT_NODE_BG)
                .stroke(Stroke::new(1.0, REACT_GRID))
                .shadow(egui::Shadow::NONE),
        ),
        header_frame: Some(
            Frame::new()
                .inner_margin(Margin {
                    left: 8,
                    right: 8,
                    top: 4,
                    bottom: 4,
                })
                .corner_radius(egui::CornerRadius::same(4))
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE),
        ),
        min_scale: Some(0.35),
        max_scale: Some(2.0),
        centering: Some(false),
        ..SnarlStyle::new()
    }
}

/// `SnarlViewer` del editor de nodos de escena/entidad.
pub(crate) struct VisualViewer {
    pub(crate) locale: Locale,
    pub(crate) context: GraphContext,
    /// Contador para ids únicos; se devuelve al estado al terminar el frame.
    pub(crate) seq: u64,
}

impl SnarlViewer<VisualNode> for VisualViewer {
    fn title(&mut self, node: &VisualNode) -> String {
        node.kind.label(self.locale).to_string()
    }

    fn inputs(&mut self, node: &VisualNode) -> usize {
        pin_layout(node.kind).0.len()
    }

    fn outputs(&mut self, node: &VisualNode) -> usize {
        pin_layout(node.kind).1.len()
    }

    fn node_frame(
        &mut self,
        default: Frame,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        snarl: &Snarl<VisualNode>,
    ) -> Frame {
        let color = category_color(snarl[node].kind.category());
        default.stroke(Stroke::new(1.0, color))
    }

    fn header_frame(
        &mut self,
        default: Frame,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        snarl: &Snarl<VisualNode>,
    ) -> Frame {
        let color = category_color(snarl[node].kind.category());
        // El header de React es una franja sólida del color de la categoría.
        default.fill(color)
    }

    fn show_header(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<VisualNode>,
    ) {
        let label = snarl[node].kind.label(self.locale);
        ui.label(
            RichText::new(label)
                .size(12.5)
                .strong()
                .color(Color32::WHITE),
        );
    }

    fn has_body(&mut self, node: &VisualNode) -> bool {
        !node_summary(node, self.locale).is_empty()
    }

    fn show_body(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<VisualNode>,
    ) {
        let summary = node_summary(&snarl[node], self.locale);
        ui.label(RichText::new(summary).size(12.0).color(REACT_TEXT));
    }

    #[allow(refining_impl_trait)]
    fn show_input(&mut self, pin: &InPin, ui: &mut Ui, snarl: &mut Snarl<VisualNode>) -> PinInfo {
        let logical = pin_layout(snarl[pin.id.node].kind)
            .0
            .get(pin.id.input)
            .copied()
            .unwrap_or(Pin::ExecIn);
        ui.label(RichText::new("").size(1.0))
            .on_hover_text(logical.tooltip(self.locale));
        PinInfo::circle().with_fill(logical.color())
    }

    #[allow(refining_impl_trait)]
    fn show_output(&mut self, pin: &OutPin, ui: &mut Ui, snarl: &mut Snarl<VisualNode>) -> PinInfo {
        let node = snarl[pin.id.node].kind;
        let logical = pin_layout(node)
            .1
            .get(pin.id.output)
            .copied()
            .unwrap_or(Pin::Exec);
        let marker = match logical {
            Pin::Then0 => "0",
            Pin::Then1 => "1",
            Pin::ThenTrue => "T",
            Pin::ThenFalse => "F",
            Pin::LoopBody => "↻",
            Pin::Exec => "▶",
            Pin::ExecIn => "",
        };
        ui.label(RichText::new(marker).size(10.0).color(logical.color()))
            .on_hover_text(logical.tooltip(self.locale));
        PinInfo::circle().with_fill(logical.color())
    }

    fn has_graph_menu(&mut self, _pos: egui::Pos2, _snarl: &mut Snarl<VisualNode>) -> bool {
        true
    }

    fn show_graph_menu(&mut self, pos: egui::Pos2, ui: &mut Ui, snarl: &mut Snarl<VisualNode>) {
        use crate::editor::i18n::tr;
        ui.label(
            RichText::new(tr(self.locale, "Añadir nodo", "Add node"))
                .size(12.0)
                .color(theme::TEXT_MUTED),
        );
        ui.separator();
        for kind in NodeKind::ALL {
            if !kind.available_in(self.context) {
                continue;
            }
            let color = category_color(kind.category());
            let label = format!("{}{}", icons::PLUS, kind.label(self.locale));
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(190.0, 26.0), egui::Sense::click());
            let hovered = response.hovered();
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(4),
                if hovered {
                    color.linear_multiply(0.75)
                } else {
                    color
                },
            );
            ui.painter().text(
                rect.left_center() + Vec2::new(8.0, 0.0),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );
            if response.clicked() {
                self.seq = self.seq.wrapping_add(1);
                let uid = format!("n_rust_{}_{}", chrono_stamp(), self.seq);
                snarl.insert_node(pos, VisualNode::new(kind, uid, kind.default_data()));
                ui.close();
            }
        }
    }

    fn has_node_menu(&mut self, _node: &VisualNode) -> bool {
        true
    }

    fn show_node_menu(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<VisualNode>,
    ) {
        use crate::editor::i18n::tr;
        if ui
            .button(tr(self.locale, "Eliminar nodo", "Remove node"))
            .clicked()
        {
            snarl.remove_node(node);
            ui.close();
        }
    }
}

// Los iconos se pintan a mano en el menú para no depender del contexto de la
// app dentro del viewer (egui-phosphor ya está cargado en la fuente global).
mod icons {
    pub const PLUS: &str = "+ ";
}
