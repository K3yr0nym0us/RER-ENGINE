//! Compilador del grafo a Rhai y validación, espejo de `compileGraphToRhai.ts`.
//!
//! La salida es byte a byte equivalente a la del renderer para que el motor
//! reciba exactamente el mismo script de escena/entidad que antes.

use std::collections::BTreeSet;

use super::{GraphContext, GraphDocument, NodeId, NodeKind, Pin, VisualNode, pin_layout};

fn escape_rhai(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn rhai_expr(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn entity_id_expr(doc: &GraphDocument, entity_id: u64) -> String {
    let owner = doc.entity_id.unwrap_or(0);
    if doc.context == GraphContext::Entity && (entity_id == 0 || entity_id == owner) {
        "entity.id".to_string()
    } else {
        entity_id.to_string()
    }
}

/// Busca el nodo al que lleva un handle de salida (`nextExecTarget`).
fn next_target(doc: &GraphDocument, from: NodeId, pin: Pin) -> Option<NodeId> {
    let (_, outputs) = pin_layout(doc.graph[from].kind);
    let output = outputs.iter().position(|p| *p == pin)?;
    doc.graph.wires().find_map(|(out, into)| {
        if out.node == from && out.output == output {
            let (inputs, _) = pin_layout(doc.graph[into.node].kind);
            if inputs.get(into.input) == Some(&Pin::ExecIn) {
                Some(into.node)
            } else {
                None
            }
        } else {
            None
        }
    })
}

fn indent_block(lines: &[String]) -> Vec<String> {
    lines.iter().map(|line| format!("    {line}")).collect()
}

fn compile_chain(
    doc: &GraphDocument,
    start: Option<NodeId>,
    visited: &mut BTreeSet<NodeId>,
    lines: &mut Vec<String>,
    scene_ids: &[u64],
) {
    let mut current = start;
    while let Some(node) = current {
        if !visited.insert(node) {
            break;
        }
        let VisualNode { kind, data, .. } = &doc.graph[node];
        match kind {
            NodeKind::Log => {
                lines.push(format!("engine.log(\"{}\");", escape_rhai(&data.message)));
                current = next_target(doc, node, Pin::Exec);
            }
            NodeKind::PlayAnimation => {
                lines.push(format!(
                    "engine.play_animation({}, \"{}\");",
                    entity_id_expr(doc, data.entity_id),
                    escape_rhai(data.animation_name.trim())
                ));
                current = next_target(doc, node, Pin::Exec);
            }
            NodeKind::SetScale => {
                lines.push(format!(
                    "engine.set_scale(entity.id, {}, {});",
                    rhai_expr(&data.scale_x, "1.0"),
                    rhai_expr(&data.scale_y, "1.0")
                ));
                current = next_target(doc, node, Pin::Exec);
            }
            NodeKind::MoveTo => {
                lines.push(format!(
                    "engine.move_to(entity.id, {}, {});",
                    rhai_expr(&data.x, "entity.x"),
                    rhai_expr(&data.y, "entity.y")
                ));
                current = next_target(doc, node, Pin::Exec);
            }
            NodeKind::Translate => {
                lines.push(format!(
                    "engine.translate(entity.id, {}, {});",
                    rhai_expr(&data.dx, "0.0"),
                    rhai_expr(&data.dy, "0.0")
                ));
                current = next_target(doc, node, Pin::Exec);
            }
            NodeKind::SetReflectionTier => {
                let tier = normalize_tier(&data.tier);
                lines.push(format!("engine.set_reflection_tier(\"{tier}\");"));
                current = next_target(doc, node, Pin::Exec);
            }
            NodeKind::Sequence => {
                compile_chain(
                    doc,
                    next_target(doc, node, Pin::Then0),
                    visited,
                    lines,
                    scene_ids,
                );
                compile_chain(
                    doc,
                    next_target(doc, node, Pin::Then1),
                    visited,
                    lines,
                    scene_ids,
                );
                current = next_target(doc, node, Pin::Exec);
            }
            NodeKind::If => {
                let expr = if data.expression.trim().is_empty() {
                    "true".to_string()
                } else {
                    data.expression.trim().to_string()
                };
                let mut true_body = Vec::new();
                compile_chain(
                    doc,
                    next_target(doc, node, Pin::ThenTrue),
                    visited,
                    &mut true_body,
                    scene_ids,
                );
                let mut false_body = Vec::new();
                compile_chain(
                    doc,
                    next_target(doc, node, Pin::ThenFalse),
                    visited,
                    &mut false_body,
                    scene_ids,
                );
                let true_body = indent_block(&true_body);
                lines.push(format!("if {expr} {{"));
                lines.extend(true_body);
                if !false_body.is_empty() {
                    lines.push("} else {".to_string());
                    lines.extend(indent_block(&false_body));
                }
                lines.push("}".to_string());
                current = next_target(doc, node, Pin::Exec);
            }
            NodeKind::ForRepeat => {
                let count = data.count;
                let mut body = Vec::new();
                compile_chain(
                    doc,
                    next_target(doc, node, Pin::LoopBody),
                    visited,
                    &mut body,
                    scene_ids,
                );
                lines.push(format!("for _i in 0..{count} {{"));
                lines.extend(indent_block(&body));
                lines.push("}".to_string());
                current = next_target(doc, node, Pin::Exec);
            }
            NodeKind::ForEachEntity => {
                let ids = if data.entity_ids.is_empty() {
                    scene_ids.to_vec()
                } else {
                    data.entity_ids.clone()
                };
                let mut body = Vec::new();
                compile_chain(
                    doc,
                    next_target(doc, node, Pin::LoopBody),
                    visited,
                    &mut body,
                    scene_ids,
                );
                let ids: Vec<String> = ids.iter().map(u64::to_string).collect();
                lines.push(format!("for entity_id in [{}] {{", ids.join(", ")));
                let mut inner = vec!["let entity = #{ id: entity_id };".to_string()];
                inner.extend(body);
                lines.extend(indent_block(&inner));
                lines.push("}".to_string());
                current = next_target(doc, node, Pin::Exec);
            }
            // Los eventos no se compilan como cuerpo: son las raíces.
            _ => current = next_target(doc, node, Pin::Exec),
        }
    }
}

fn normalize_tier(tier: &str) -> &str {
    match tier {
        "low" | "medium" | "high" => tier,
        _ => "off",
    }
}

fn event_body(doc: &GraphDocument, event: NodeId, scene_ids: &[u64]) -> String {
    let mut visited = BTreeSet::new();
    let mut lines = Vec::new();
    compile_chain(
        doc,
        next_target(doc, event, Pin::Exec),
        &mut visited,
        &mut lines,
        scene_ids,
    );
    if lines.is_empty() {
        "    // (empty)\n".to_string()
    } else {
        format!(
            "{}\n",
            lines
                .iter()
                .map(|line| format!("    {line}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

fn find_event(doc: &GraphDocument, kind: NodeKind) -> Option<NodeId> {
    doc.graph
        .node_ids()
        .find(|(_, node)| node.kind == kind)
        .map(|(id, _)| id)
}

/// Compila el grafo a Rhai. Devuelve los mismos errores que `validateGraph`.
pub(crate) fn compile(doc: &GraphDocument, scene_ids: &[u64]) -> Result<String, Vec<String>> {
    let issues = validate(doc);
    if !issues.is_empty() {
        return Err(issues);
    }

    let source = if doc.context == GraphContext::Entity {
        let start = find_event(doc, NodeKind::EntityStart);
        let update = find_event(doc, NodeKind::EntityUpdate);
        let start_body = start
            .map(|id| event_body(doc, id, scene_ids))
            .unwrap_or_else(|| "    // (empty)\n".to_string());
        let update_body = update
            .map(|id| event_body(doc, id, scene_ids))
            .unwrap_or_else(|| "    // (empty)\n".to_string());
        format!(
            "// RER — entity logic (compiled from nodes)\n\
             // entity_id: {}\n\
             \n\
             fn on_start(entity) {{\n\
             {start_body}}}\n\
             \n\
             fn update(entity, dt) {{\n\
             {update_body}}}\n",
            doc.entity_id.unwrap_or(0)
        )
    } else {
        let begin = find_event(doc, NodeKind::SceneBegin);
        let tick = find_event(doc, NodeKind::Tick);
        let start_body = begin
            .map(|id| event_body(doc, id, scene_ids))
            .unwrap_or_else(|| "    // (empty)\n".to_string());
        let tick_body = tick
            .map(|id| event_body(doc, id, scene_ids))
            .unwrap_or_else(|| "    // (empty)\n".to_string());
        format!(
            "// RER — scene logic (compiled from nodes)\n\
             // scene_id: {}\n\
             \n\
             fn on_scene_start() {{\n\
             {start_body}}}\n\
             \n\
             fn on_scene_tick(dt) {{\n\
             {tick_body}}}\n",
            doc.scene_id.unwrap_or(0)
        )
    };
    Ok(source)
}

// ---------------------------------------------------------------------------
// Validación (`validateGraph.ts`)
// ---------------------------------------------------------------------------

fn event_types(context: GraphContext) -> [NodeKind; 2] {
    match context {
        GraphContext::Scene => [NodeKind::SceneBegin, NodeKind::Tick],
        GraphContext::Entity => [NodeKind::EntityStart, NodeKind::EntityUpdate],
    }
}

fn visit_subtree(
    doc: &GraphDocument,
    node: Option<NodeId>,
    visited: &mut BTreeSet<NodeId>,
    issues: &mut Vec<String>,
) {
    let Some(node) = node else { return };
    if !visited.insert(node) {
        issues.push("Cycle detected in execution flow".to_string());
        return;
    }
    let kind = doc.graph[node].kind;
    match kind {
        NodeKind::Sequence => {
            visit_subtree(doc, next_target(doc, node, Pin::Then0), visited, issues);
            visit_subtree(doc, next_target(doc, node, Pin::Then1), visited, issues);
        }
        NodeKind::If => {
            visit_subtree(doc, next_target(doc, node, Pin::ThenTrue), visited, issues);
            visit_subtree(doc, next_target(doc, node, Pin::ThenFalse), visited, issues);
        }
        NodeKind::ForRepeat | NodeKind::ForEachEntity => {
            visit_subtree(doc, next_target(doc, node, Pin::LoopBody), visited, issues);
        }
        _ => {}
    }
    visit_subtree(doc, next_target(doc, node, Pin::Exec), visited, issues);
}

pub(crate) fn validate(doc: &GraphDocument) -> Vec<String> {
    let mut issues = Vec::new();
    if doc.context == GraphContext::Scene && doc.scene_id.unwrap_or(0) == 0 {
        issues.push("Invalid scene id".to_string());
    }
    if doc.context == GraphContext::Entity && doc.entity_id.unwrap_or(0) == 0 {
        issues.push("Invalid entity id".to_string());
    }

    let events = event_types(doc.context);
    for kind in events {
        let matches = doc
            .graph
            .node_ids()
            .filter(|(_, node)| node.kind == kind)
            .count();
        if matches > 1 {
            let label = match kind {
                NodeKind::SceneBegin => "Scene start",
                NodeKind::EntityStart => "On start",
                _ => "Each frame",
            };
            issues.push(format!("Only one {label} node allowed"));
        }
    }

    for (out, into) in doc.graph.wires() {
        if doc.graph.get_node(out.node).is_none() || doc.graph.get_node(into.node).is_none() {
            issues.push(format!(
                "Orphan edge: e_{}_{}_{}",
                out.node.0, out.output, into.node.0
            ));
        }
    }

    let mut visited = BTreeSet::new();
    for event in events {
        for id in doc
            .graph
            .node_ids()
            .filter(|(_, node)| node.kind == event)
            .map(|(id, _)| id)
        {
            visit_subtree(
                doc,
                next_target(doc, id, Pin::Exec),
                &mut visited,
                &mut issues,
            );
        }
    }

    for (id, node) in doc.graph.node_ids() {
        let data = &node.data;
        let uid = &node.uid;
        match node.kind {
            NodeKind::Log if data.message.trim().is_empty() => {
                issues.push(format!("Print node {uid}: empty message"));
            }
            NodeKind::If if data.expression.trim().is_empty() => {
                issues.push(format!("Branch node {uid}: empty condition"));
            }
            NodeKind::ForEachEntity if doc.context != GraphContext::Scene => {
                issues.push("For each entity is only available in scene logic".to_string());
            }
            NodeKind::SetScale | NodeKind::MoveTo | NodeKind::Translate
                if doc.context != GraphContext::Entity =>
            {
                issues.push(format!(
                    "{} is only available in entity logic",
                    node.kind.as_str()
                ));
            }
            NodeKind::SetScale
                if data.scale_x.trim().is_empty() || data.scale_y.trim().is_empty() =>
            {
                issues.push(format!("Set scale node {uid}: empty value"));
            }
            NodeKind::MoveTo if data.x.trim().is_empty() || data.y.trim().is_empty() => {
                issues.push(format!("Teleport to node {uid}: empty value"));
            }
            NodeKind::Translate if data.dx.trim().is_empty() || data.dy.trim().is_empty() => {
                issues.push(format!("Translate node {uid}: empty value"));
            }
            NodeKind::SetReflectionTier if doc.context != GraphContext::Scene => {
                issues.push("Set reflection tier is only available in scene logic".to_string());
            }
            NodeKind::PlayAnimation => {
                let owner = doc.entity_id.unwrap_or(0);
                let resolved = if doc.context == GraphContext::Entity && data.entity_id == 0 {
                    owner
                } else {
                    data.entity_id
                };
                if resolved == 0 {
                    issues.push(format!("Play animation node {uid}: select entity"));
                }
                if data.animation_name.trim().is_empty() {
                    issues.push(format!("Play animation node {uid}: select animation"));
                }
            }
            _ => {}
        }

        if events.contains(&node.kind) {
            continue;
        }
        let has_exec_in = doc.graph.wires().any(|(_, into)| {
            into.node == id
                && pin_layout(doc.graph[into.node].kind).0.get(into.input) == Some(&Pin::ExecIn)
        });
        if !has_exec_in && node.kind != NodeKind::Sequence {
            issues.push(format!("Node {uid} has no execution input"));
        }
    }
    issues
}

/// Traduce los mensajes de validación para la UI nativa.
pub(crate) fn localize_issue(issue: &str, locale: crate::editor::cli::Locale) -> String {
    use crate::editor::cli::Locale;
    if locale == Locale::En {
        return issue.to_string();
    }
    if issue == "Cycle detected in execution flow" {
        return "Ciclo detectado en el flujo de ejecución".to_string();
    }
    if issue == "Invalid scene id" {
        return "Id de escena inválido".to_string();
    }
    if issue == "Invalid entity id" {
        return "Id de entidad inválido".to_string();
    }
    if issue.starts_with("Only one ") {
        return format!(
            "Solo se permite un nodo {}",
            issue.trim_start_matches("Only one ")
        );
    }
    if issue.ends_with(": empty message") {
        return format!(
            "Nodo {}: mensaje vacío",
            issue.trim_end_matches(": empty message")
        );
    }
    if issue.ends_with(": empty condition") {
        return format!(
            "Nodo {}: condición vacía",
            issue.trim_end_matches(": empty condition")
        );
    }
    if issue.ends_with(": empty value") {
        return format!(
            "Nodo {}: valor vacío",
            issue.trim_end_matches(": empty value")
        );
    }
    if issue.ends_with(": select entity") {
        return format!(
            "Nodo {}: selecciona una entidad",
            issue.trim_end_matches(": select entity")
        );
    }
    if issue.ends_with(": select animation") {
        return format!(
            "Nodo {}: selecciona una animación",
            issue.trim_end_matches(": select animation")
        );
    }
    if issue.contains("has no execution input") {
        return format!(
            "El nodo {} no tiene entrada de ejecución",
            issue.split_whitespace().nth(1).unwrap_or("")
        );
    }
    issue.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::visual_script::{GraphDocument, NodeData, VisualNode};

    #[test]
    fn empty_scene_matches_react_skeleton() {
        let doc = GraphDocument::empty(GraphContext::Scene, Some(1), None);
        let source = compile(&doc, &[]).expect("grafo vacío válido");
        assert_eq!(
            source,
            "// RER — scene logic (compiled from nodes)\n\
             // scene_id: 1\n\
             \n\
             fn on_scene_start() {\n\
             \x20   // (empty)\n\
             }\n\
             \n\
             fn on_scene_tick(dt) {\n\
             \x20   // (empty)\n\
             }\n"
        );
    }

    #[test]
    fn scene_event_chain_compiles_log() {
        let mut doc = GraphDocument::empty(GraphContext::Scene, Some(1), None);
        let event = doc.graph.insert_node(
            egui::pos2(0.0, 0.0),
            VisualNode::new(NodeKind::SceneBegin, "event".into(), NodeData::default()),
        );
        let data = NodeData {
            message: "Hola \"mundo\"".into(),
            ..Default::default()
        };
        let log = doc.graph.insert_node(
            egui::pos2(200.0, 0.0),
            VisualNode::new(NodeKind::Log, "log".into(), data),
        );
        doc.graph.connect(
            egui_snarl::OutPinId {
                node: event,
                output: 0,
            },
            egui_snarl::InPinId {
                node: log,
                input: 0,
            },
        );
        let source = compile(&doc, &[]).expect("grafo válido");
        assert!(source.contains("engine.log(\"Hola \\\"mundo\\\"\");"));
        assert!(validate(&doc).is_empty());
    }

    #[test]
    fn empty_log_message_is_rejected() {
        let mut doc = GraphDocument::empty(GraphContext::Scene, Some(1), None);
        let event = doc.graph.insert_node(
            egui::pos2(0.0, 0.0),
            VisualNode::new(NodeKind::SceneBegin, "event".into(), NodeData::default()),
        );
        let log = doc.graph.insert_node(
            egui::pos2(200.0, 0.0),
            VisualNode::new(NodeKind::Log, "log".into(), NodeData::default()),
        );
        doc.graph.connect(
            egui_snarl::OutPinId {
                node: event,
                output: 0,
            },
            egui_snarl::InPinId {
                node: log,
                input: 0,
            },
        );
        let errors = compile(&doc, &[]).expect_err("mensaje vacío");
        assert!(errors.iter().any(|error| error.contains("empty message")));
    }

    #[test]
    fn entity_logic_uses_entity_id_expression() {
        let mut doc = GraphDocument::empty(GraphContext::Entity, None, Some(7));
        let event = doc.graph.insert_node(
            egui::pos2(0.0, 0.0),
            VisualNode::new(NodeKind::EntityStart, "event".into(), NodeData::default()),
        );
        let mut data = NodeKind::PlayAnimation.default_data();
        data.animation_name = "Run".into();
        let anim = doc.graph.insert_node(
            egui::pos2(200.0, 0.0),
            VisualNode::new(NodeKind::PlayAnimation, "anim".into(), data),
        );
        doc.graph.connect(
            egui_snarl::OutPinId {
                node: event,
                output: 0,
            },
            egui_snarl::InPinId {
                node: anim,
                input: 0,
            },
        );
        let source = compile(&doc, &[]).expect("grafo válido");
        assert!(source.contains("fn on_start(entity)"));
        assert!(source.contains("engine.play_animation(entity.id, \"Run\");"));
    }
}
