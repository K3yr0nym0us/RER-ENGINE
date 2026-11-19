//! Modelo canónico del editor visual de lógica (Level/Entity Blueprint).
//!
//! Es el espejo en Rust de `src/renderer/src/visualScripting/`: el documento
//! serializado es exactamente el mismo `VisualGraphDocument` que usaba React
//! Flow, de modo que los `.save` siguen siendo compatibles en ambos sentidos.
//! El grafo en memoria es un `egui_snarl::Snarl<VisualNode>`.

use std::collections::{BTreeMap, BTreeSet};

use egui_snarl::{InPinId, NodeId, OutPinId, Snarl};
use serde_json::{Value, json};

pub(crate) mod compile;
pub(crate) mod viewer;

/// Contexto del grafo, equivalente a `VisualGraphContext`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GraphContext {
    Scene,
    Entity,
}

impl GraphContext {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Scene => "scene",
            Self::Entity => "entity",
        }
    }
}

/// Los 14 tipos de nodo del MVP, con el mismo identificador que React.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeKind {
    SceneBegin,
    Tick,
    EntityStart,
    EntityUpdate,
    Sequence,
    If,
    ForRepeat,
    ForEachEntity,
    Log,
    PlayAnimation,
    SetScale,
    MoveTo,
    Translate,
    SetReflectionTier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeCategory {
    Event,
    Flow,
    Action,
}

impl NodeKind {
    pub(crate) const ALL: [Self; 14] = [
        Self::SceneBegin,
        Self::Tick,
        Self::EntityStart,
        Self::EntityUpdate,
        Self::Sequence,
        Self::If,
        Self::ForRepeat,
        Self::ForEachEntity,
        Self::Log,
        Self::PlayAnimation,
        Self::SetScale,
        Self::MoveTo,
        Self::Translate,
        Self::SetReflectionTier,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::SceneBegin => "event.scene_begin",
            Self::Tick => "event.tick",
            Self::EntityStart => "event.entity_start",
            Self::EntityUpdate => "event.entity_update",
            Self::Sequence => "flow.sequence",
            Self::If => "flow.if",
            Self::ForRepeat => "flow.for_repeat",
            Self::ForEachEntity => "flow.for_each_entity",
            Self::Log => "action.log",
            Self::PlayAnimation => "action.play_animation",
            Self::SetScale => "action.set_scale",
            Self::MoveTo => "action.move_to",
            Self::Translate => "action.translate",
            Self::SetReflectionTier => "action.set_reflection_tier",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }

    /// Etiqueta bilingüe (las claves i18n de React se traducen aquí igual que
    /// en el resto de ventanas nativas).
    pub(crate) fn label(self, locale: crate::editor::cli::Locale) -> &'static str {
        use crate::editor::cli::Locale;
        match (self, locale) {
            (Self::SceneBegin, Locale::Es) => "Inicio de escena",
            (Self::SceneBegin, _) => "Scene start",
            (Self::Tick, Locale::Es) => "Cada frame",
            (Self::Tick, _) => "Each frame",
            (Self::EntityStart, Locale::Es) => "Al iniciar",
            (Self::EntityStart, _) => "On start",
            (Self::EntityUpdate, Locale::Es) => "Cada frame",
            (Self::EntityUpdate, _) => "Each frame",
            (Self::Sequence, Locale::Es) => "Secuencia",
            (Self::Sequence, _) => "Sequence",
            (Self::If, Locale::Es) => "Bifurcación (if)",
            (Self::If, _) => "Branch (if)",
            (Self::ForRepeat, Locale::Es) => "Repetir N veces",
            (Self::ForRepeat, _) => "Repeat N times",
            (Self::ForEachEntity, Locale::Es) => "Para cada entidad",
            (Self::ForEachEntity, _) => "For each entity",
            (Self::Log, Locale::Es) => "Imprimir",
            (Self::Log, _) => "Print",
            (Self::PlayAnimation, Locale::Es) => "Reproducir animación",
            (Self::PlayAnimation, _) => "Play animation",
            (Self::SetScale, Locale::Es) => "Escalar",
            (Self::SetScale, _) => "Set scale",
            (Self::MoveTo, Locale::Es) => "Teletransportar",
            (Self::MoveTo, _) => "Teleport to",
            (Self::Translate, Locale::Es) => "Trasladar",
            (Self::Translate, _) => "Translate",
            (Self::SetReflectionTier, Locale::Es) => "Nivel de reflexión",
            (Self::SetReflectionTier, _) => "Set reflection tier",
        }
    }

    pub(crate) fn category(self) -> NodeCategory {
        match self {
            Self::SceneBegin | Self::Tick | Self::EntityStart | Self::EntityUpdate => {
                NodeCategory::Event
            }
            Self::Sequence | Self::If | Self::ForRepeat | Self::ForEachEntity => NodeCategory::Flow,
            _ => NodeCategory::Action,
        }
    }

    pub(crate) fn available_in(self, context: GraphContext) -> bool {
        match self {
            Self::SceneBegin | Self::Tick | Self::ForEachEntity | Self::SetReflectionTier => {
                context == GraphContext::Scene
            }
            Self::EntityStart
            | Self::EntityUpdate
            | Self::SetScale
            | Self::MoveTo
            | Self::Translate => context == GraphContext::Entity,
            _ => true,
        }
    }

    pub(crate) fn is_event(self) -> bool {
        self.category() == NodeCategory::Event
    }

    /// Datos con los que React crea el nodo (`defaultData`).
    pub(crate) fn default_data(self) -> NodeData {
        let mut data = NodeData::default();
        match self {
            Self::If => data.expression = "true".into(),
            Self::ForRepeat => data.count = 3,
            Self::Log => data.message = "Hello from logic".into(),
            Self::PlayAnimation => {
                data.entity_id = 0;
                data.animation_name.clear();
            }
            Self::SetScale => {
                data.scale_x = "1.0".into();
                data.scale_y = "1.0".into();
            }
            Self::MoveTo => {
                data.x = "entity.x".into();
                data.y = "entity.y".into();
            }
            Self::Translate => {
                data.dx = "0.0".into();
                data.dy = "0.0".into();
            }
            Self::SetReflectionTier => data.tier = "off".into(),
            _ => {}
        }
        data
    }
}

/// Campos editables de un nodo (espejo de `VisualScriptNodeData`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NodeData {
    pub(crate) message: String,
    pub(crate) expression: String,
    pub(crate) count: u64,
    pub(crate) entity_ids: Vec<u64>,
    pub(crate) entity_id: u64,
    pub(crate) animation_name: String,
    pub(crate) scale_x: String,
    pub(crate) scale_y: String,
    pub(crate) x: String,
    pub(crate) y: String,
    pub(crate) dx: String,
    pub(crate) dy: String,
    pub(crate) tier: String,
}

impl Default for NodeData {
    fn default() -> Self {
        Self {
            message: String::new(),
            expression: String::new(),
            count: 1,
            entity_ids: Vec::new(),
            entity_id: 0,
            animation_name: String::new(),
            scale_x: String::new(),
            scale_y: String::new(),
            x: String::new(),
            y: String::new(),
            dx: String::new(),
            dy: String::new(),
            tier: String::new(),
        }
    }
}

impl NodeData {
    /// Serialización canónica: solo los campos que React escribe en `defaultData`.
    pub(crate) fn to_value(&self, kind: NodeKind) -> Value {
        match kind {
            NodeKind::If => json!({ "expression": self.expression }),
            NodeKind::ForRepeat => json!({ "count": self.count }),
            NodeKind::ForEachEntity => json!({
                "entityIds": self.entity_ids.iter().map(|id| json!(id)).collect::<Vec<_>>()
            }),
            NodeKind::Log => json!({ "message": self.message }),
            NodeKind::PlayAnimation => json!({
                "entityId": self.entity_id,
                "animationName": self.animation_name,
            }),
            NodeKind::SetScale => json!({ "scaleX": self.scale_x, "scaleY": self.scale_y }),
            NodeKind::MoveTo => json!({ "x": self.x, "y": self.y }),
            NodeKind::Translate => json!({ "dx": self.dx, "dy": self.dy }),
            NodeKind::SetReflectionTier => json!({ "tier": self.tier }),
            _ => json!({}),
        }
    }

    pub(crate) fn from_value(kind: NodeKind, data: Option<&Value>) -> Self {
        let mut out = kind.default_data();
        let Some(data) = data else {
            return out;
        };
        let str_field = |key: &str| -> Option<String> {
            data.get(key).and_then(Value::as_str).map(str::to_string)
        };
        let u64_field = |key: &str| -> Option<u64> { data.get(key).and_then(Value::as_u64) };
        match kind {
            NodeKind::If => {
                if let Some(value) = str_field("expression") {
                    out.expression = value;
                }
            }
            NodeKind::ForRepeat => {
                if let Some(value) = u64_field("count") {
                    out.count = value;
                } else if let Some(value) = data.get("count").and_then(Value::as_i64) {
                    out.count = value.max(0) as u64;
                }
            }
            NodeKind::ForEachEntity => {
                if let Some(ids) = data.get("entityIds").and_then(Value::as_array) {
                    out.entity_ids = ids.iter().filter_map(Value::as_u64).collect();
                }
            }
            NodeKind::Log => {
                if let Some(value) = str_field("message") {
                    out.message = value;
                }
            }
            NodeKind::PlayAnimation => {
                if let Some(value) = u64_field("entityId") {
                    out.entity_id = value;
                }
                if let Some(value) = str_field("animationName") {
                    out.animation_name = value;
                }
            }
            NodeKind::SetScale => {
                if let Some(value) = str_field("scaleX") {
                    out.scale_x = value;
                }
                if let Some(value) = str_field("scaleY") {
                    out.scale_y = value;
                }
            }
            NodeKind::MoveTo => {
                if let Some(value) = str_field("x") {
                    out.x = value;
                }
                if let Some(value) = str_field("y") {
                    out.y = value;
                }
            }
            NodeKind::Translate => {
                if let Some(value) = str_field("dx") {
                    out.dx = value;
                }
                if let Some(value) = str_field("dy") {
                    out.dy = value;
                }
            }
            NodeKind::SetReflectionTier => {
                if let Some(value) = str_field("tier") {
                    out.tier = value;
                }
            }
            _ => {}
        }
        out
    }
}

/// Nodo dentro de `Snarl`. El `uid` es el id canónico del documento React.
#[derive(Debug, Clone)]
pub(crate) struct VisualNode {
    pub(crate) uid: String,
    pub(crate) kind: NodeKind,
    pub(crate) data: NodeData,
}

impl VisualNode {
    pub(crate) fn new(kind: NodeKind, uid: String, data: NodeData) -> Self {
        Self { uid, kind, data }
    }
}

/// Handle lógico de un pin; el orden del vec determina el índice en snarl.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pin {
    ExecIn,
    Exec,
    Then0,
    Then1,
    ThenTrue,
    ThenFalse,
    LoopBody,
}

impl Pin {
    pub(crate) fn handle(self) -> &'static str {
        match self {
            Self::ExecIn => "exec_in",
            Self::Exec => "exec",
            Self::Then0 => "then_0",
            Self::Then1 => "then_1",
            Self::ThenTrue => "then_true",
            Self::ThenFalse => "then_false",
            Self::LoopBody => "loop_body",
        }
    }

    /// Color del pin, igual que el `style.background` de los handles React.
    pub(crate) fn color(self) -> egui::Color32 {
        use egui::Color32;
        match self {
            Self::ThenTrue => Color32::from_rgb(0x20, 0xc9, 0x97),
            Self::ThenFalse => Color32::from_rgb(0xdc, 0x35, 0x45),
            Self::LoopBody => Color32::from_rgb(0xff, 0xc1, 0x07),
            _ => Color32::from_rgb(0xad, 0xb5, 0xbd),
        }
    }

    pub(crate) fn tooltip(self, locale: crate::editor::cli::Locale) -> &'static str {
        use crate::editor::cli::Locale;
        match (self, locale) {
            (Self::ExecIn, Locale::Es) => "Entrada de ejecución",
            (Self::Exec, Locale::Es) => "Salida de ejecución",
            (Self::Then0, Locale::Es) => "Entonces 0",
            (Self::Then1, Locale::Es) => "Entonces 1",
            (Self::ThenTrue, Locale::Es) => "Verdadero",
            (Self::ThenFalse, Locale::Es) => "Falso",
            (Self::LoopBody, Locale::Es) => "Cuerpo del bucle",
            (Self::ExecIn, _) => "Execution input",
            (Self::Exec, _) => "Execution output",
            (Self::Then0, _) => "Then 0",
            (Self::Then1, _) => "Then 1",
            (Self::ThenTrue, _) => "Then true",
            (Self::ThenFalse, _) => "Then false",
            (Self::LoopBody, _) => "Loop body",
        }
    }
}

/// Entradas y salidas de cada nodo, en el mismo orden que el JSX de React.
pub(crate) fn pin_layout(kind: NodeKind) -> (Vec<Pin>, Vec<Pin>) {
    let inputs = if kind.is_event() {
        Vec::new()
    } else {
        vec![Pin::ExecIn]
    };
    let outputs = match kind {
        NodeKind::Sequence => vec![Pin::Then0, Pin::Then1, Pin::Exec],
        NodeKind::If => vec![Pin::ThenTrue, Pin::ThenFalse, Pin::Exec],
        NodeKind::ForRepeat | NodeKind::ForEachEntity => vec![Pin::LoopBody, Pin::Exec],
        _ => vec![Pin::Exec],
    };
    (inputs, outputs)
}

/// Texto del cuerpo del nodo (el `<div className="px-2 py-2 small">` de React).
pub(crate) fn node_summary(node: &VisualNode, locale: crate::editor::cli::Locale) -> String {
    use crate::editor::cli::Locale;
    let d = &node.data;
    let tr = |es: &'static str, en: &'static str| match locale {
        Locale::Es => es,
        Locale::En => en,
    };
    match node.kind {
        NodeKind::Log => d.message.clone(),
        NodeKind::PlayAnimation => {
            let prefix = if d.entity_id > 0 {
                format!("{} · ", d.entity_id)
            } else {
                String::new()
            };
            format!(
                "{}{}",
                prefix,
                if d.animation_name.is_empty() {
                    tr("Seleccionar animación", "Select animation").to_string()
                } else {
                    d.animation_name.clone()
                }
            )
        }
        NodeKind::SetScale => format!("{}, {}", d.scale_x, d.scale_y),
        NodeKind::MoveTo => format!("{}, {}", d.x, d.y),
        NodeKind::Translate => format!("Δ {}, {}", d.dx, d.dy),
        NodeKind::Sequence => tr("Entonces 0 → Entonces 1", "Then 0 → Then 1").to_string(),
        NodeKind::If => {
            if d.expression.is_empty() {
                "true".to_string()
            } else {
                d.expression.clone()
            }
        }
        NodeKind::ForRepeat => format!("{}: {}", tr("Cantidad", "Count"), d.count),
        NodeKind::ForEachEntity => {
            if d.entity_ids.is_empty() {
                tr("Todas las entidades de la escena", "All scene entities").to_string()
            } else {
                format!("{} {}", d.entity_ids.len(), tr("entidades", "entities"))
            }
        }
        kind if kind.is_event() => tr("Evento", "Event").to_string(),
        _ => String::new(),
    }
}

/// Documento canónico `VisualGraphDocument` (idéntico al formato React).
#[derive(Debug, Clone)]
pub(crate) struct GraphDocument {
    pub(crate) context: GraphContext,
    pub(crate) scene_id: Option<u64>,
    pub(crate) entity_id: Option<u64>,
    pub(crate) graph: Snarl<VisualNode>,
}

impl GraphDocument {
    pub(crate) fn empty(
        context: GraphContext,
        scene_id: Option<u64>,
        entity_id: Option<u64>,
    ) -> Self {
        Self {
            context,
            scene_id,
            entity_id,
            graph: Snarl::new(),
        }
    }

    /// Serializa al mismo JSON que `flowToGraph` producía en React.
    pub(crate) fn to_json(&self) -> Value {
        let nodes: Vec<Value> = self
            .graph
            .nodes_pos_ids()
            .map(|(_, pos, node)| {
                json!({
                    "id": node.uid,
                    "type": node.kind.as_str(),
                    "position": { "x": pos.x, "y": pos.y },
                    "data": node.data.to_value(node.kind),
                })
            })
            .collect();
        let uid_by_node: BTreeMap<NodeId, String> = self
            .graph
            .node_ids()
            .map(|(id, node)| (id, node.uid.clone()))
            .collect();
        let edges: Vec<Value> = self
            .graph
            .wires()
            .filter_map(|(from, to)| {
                let source = uid_by_node.get(&from.node)?;
                let target = uid_by_node.get(&to.node)?;
                let (_, outputs) = pin_layout(self.graph[from.node].kind);
                let (inputs, _) = pin_layout(self.graph[to.node].kind);
                let source_handle = outputs.get(from.output)?.handle();
                let target_handle = inputs.get(to.input)?.handle();
                Some(json!({
                    "id": format!("e_{}_{}_{}_{}", source, source_handle, target, target_handle),
                    "source": source,
                    "sourceHandle": source_handle,
                    "target": target,
                    "targetHandle": target_handle,
                }))
            })
            .collect();
        let mut out = json!({
            "version": 1,
            "context": self.context.as_str(),
            "nodes": nodes,
            "edges": edges,
        });
        if let Some(id) = self.scene_id {
            out["sceneId"] = json!(id);
        }
        if let Some(id) = self.entity_id {
            out["entityId"] = json!(id);
        }
        out
    }

    /// Lee el grafo canónico guardado (o construye el grafo vacío).
    pub(crate) fn from_json(
        value: Option<&Value>,
        context: GraphContext,
        scene_id: Option<u64>,
        entity_id: Option<u64>,
    ) -> Self {
        let mut doc = Self::empty(context, scene_id, entity_id);
        let Some(value) = value else {
            return doc;
        };
        let Some(nodes) = value.get("nodes").and_then(Value::as_array) else {
            return doc;
        };

        let mut node_by_uid = BTreeMap::new();
        let mut pending_edges = Vec::new();
        for node in nodes {
            let Some(uid) = node.get("id").and_then(Value::as_str) else {
                continue;
            };
            let Some(kind) = node
                .get("type")
                .and_then(Value::as_str)
                .and_then(NodeKind::parse)
            else {
                continue;
            };
            let x = node
                .get("position")
                .and_then(|p| p.get("x"))
                .and_then(Value::as_f64)
                .unwrap_or(80.0) as f32;
            let y = node
                .get("position")
                .and_then(|p| p.get("y"))
                .and_then(Value::as_f64)
                .unwrap_or(80.0) as f32;
            let data = NodeData::from_value(kind, node.get("data"));
            let snarl_id = doc.graph.insert_node(
                egui::pos2(x, y),
                VisualNode::new(kind, uid.to_string(), data),
            );
            node_by_uid.insert(uid.to_string(), snarl_id);
        }

        if let Some(edges) = value.get("edges").and_then(Value::as_array) {
            for edge in edges {
                let (Some(source), Some(target)) = (
                    edge.get("source").and_then(Value::as_str),
                    edge.get("target").and_then(Value::as_str),
                ) else {
                    continue;
                };
                pending_edges.push((
                    source.to_string(),
                    edge.get("sourceHandle")
                        .and_then(Value::as_str)
                        .unwrap_or("exec")
                        .to_string(),
                    target.to_string(),
                    edge.get("targetHandle")
                        .and_then(Value::as_str)
                        .unwrap_or("exec_in")
                        .to_string(),
                ));
            }
        }

        for (source, source_handle, target, target_handle) in pending_edges {
            let (Some(&from), Some(&to)) = (node_by_uid.get(&source), node_by_uid.get(&target))
            else {
                continue;
            };
            let (_, outputs) = pin_layout(doc.graph[from].kind);
            let (inputs, _) = pin_layout(doc.graph[to].kind);
            let Some(output) = outputs.iter().position(|pin| pin.handle() == source_handle) else {
                continue;
            };
            let Some(input) = inputs.iter().position(|pin| pin.handle() == target_handle) else {
                continue;
            };
            doc.graph
                .connect(OutPinId { node: from, output }, InPinId { node: to, input });
        }
        doc
    }

    /// Busca un nodo por su id canónico.
    pub(crate) fn node_by_uid(&self, uid: &str) -> Option<NodeId> {
        self.graph
            .node_ids()
            .find(|(_, node)| node.uid == uid)
            .map(|(id, _)| id)
    }
}

/// Marca de tiempo barata para ids únicos (sin añadir dependencias).
pub(crate) fn chrono_stamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

/// Estado de la ventana nativa del editor de nodos.
pub(crate) struct VisualScriptState {
    pub(crate) open: bool,
    pub(crate) context: GraphContext,
    pub(crate) scene_id: Option<u64>,
    pub(crate) entity_id: Option<u64>,
    pub(crate) scene_name: String,
    pub(crate) entity_name: String,
    pub(crate) doc: GraphDocument,
    pub(crate) error: Option<String>,
    pub(crate) seq: u64,
    /// El snapshot de React puede llegar después de abrir la ventana.
    pub(crate) loaded: bool,
    /// Acordeones abiertos del panel de variables.
    pub(crate) open_groups: BTreeSet<String>,
    /// Id canónico del nodo seleccionado (alimenta el inspector derecho).
    pub(crate) selected: Option<String>,
    /// Si se abre desde un tooltip, id del tooltip al que guardar el grafo.
    pub(crate) tooltip_target: Option<String>,
    /// Nombre del tooltip al que guardar el grafo.
    pub(crate) tooltip_name: String,
}

impl Default for VisualScriptState {
    fn default() -> Self {
        Self {
            open: false,
            context: GraphContext::Scene,
            scene_id: None,
            entity_id: None,
            scene_name: String::new(),
            entity_name: String::new(),
            doc: GraphDocument::empty(GraphContext::Scene, None, None),
            error: None,
            seq: 0,
            loaded: false,
            open_groups: BTreeSet::new(),
            selected: None,
            tooltip_target: None,
            tooltip_name: String::new(),
        }
    }
}

/// Construye el documento y los títulos a partir del último snapshot recibido.
pub(crate) fn load_document(
    app: &crate::editor::app::EditorApp,
) -> (GraphDocument, String, String) {
    let context = app.visual_script.context;
    let scene_id = app.visual_script.scene_id;
    let entity_id = app.visual_script.entity_id;

    let mut scene_name = String::new();
    let mut entity_name = String::new();
    // Al abrir desde un tooltip, el grafo vive en la definición del tooltip.
    if let Some(tooltip_id) = app.visual_script.tooltip_target.as_deref() {
        let graph_value = app
            .project_state
            .interaction_tooltips
            .iter()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(tooltip_id))
            .and_then(|item| item.get("action_ref_graph"));
        return (
            GraphDocument::from_json(graph_value, context, scene_id, entity_id),
            String::new(),
            app.visual_script.tooltip_name.clone(),
        );
    }
    let graph_value = match context {
        GraphContext::Scene => app
            .project_state
            .scenes
            .iter()
            .find(|scene| {
                scene_id.is_some_and(|id| scene.get("id").and_then(Value::as_u64) == Some(id))
            })
            .map(|scene| {
                scene_name = scene
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                scene.get("visualGraph")
            })
            .flatten(),
        GraphContext::Entity => {
            entity_name = entity_id
                .and_then(|id| app.project_state.entity_meta_for(id))
                .and_then(|meta| meta.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if entity_name.is_empty() {
                entity_name = app
                    .selected_entity
                    .as_ref()
                    .map(|selected| selected.name.clone())
                    .unwrap_or_default();
            }
            entity_id
                .and_then(|id| app.project_state.entity_meta_for(id))
                .and_then(|meta| meta.get("visualGraph"))
        }
    };

    (
        GraphDocument::from_json(graph_value, context, scene_id, entity_id),
        scene_name,
        entity_name,
    )
}

// ---------------------------------------------------------------------------
// Panel de variables: espejo de `contextVariables.ts`.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct VariableItem {
    pub(crate) label: String,
    pub(crate) snippet: String,
    pub(crate) detail: String,
    /// Si se pulsa, copia este texto en el portapapeles.
    pub(crate) copy: Option<String>,
    /// Al copiar una animación se rellena el nodo Play animation seleccionado.
    pub(crate) animation: Option<(u64, String)>,
}

impl VariableItem {
    fn new(label: impl Into<String>, snippet: impl Into<String>) -> Self {
        let snippet = snippet.into();
        Self {
            label: label.into(),
            copy: Some(snippet.clone()),
            snippet,
            detail: String::new(),
            animation: None,
        }
    }

    fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }
}

#[derive(Debug, Clone)]
pub(crate) struct VariableGroup {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) items: Vec<VariableItem>,
    pub(crate) nested: Vec<VariableGroup>,
    pub(crate) copy_id: Option<u64>,
}

impl VariableGroup {
    fn new(key: impl Into<String>, label: impl Into<String>, items: Vec<VariableItem>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            items,
            nested: Vec::new(),
            copy_id: None,
        }
    }
}

/// Entidad de escena ya resuelta (categoría + animaciones), como la que React
/// construye en `resolveSceneEntitiesForVisualScript`.
#[derive(Debug, Clone)]
pub(crate) struct SceneEntity {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) category: String,
    pub(crate) blueprint_id: String,
    pub(crate) animations: Vec<String>,
}

/// Extrae las entidades de escena del último snapshot de React.
pub(crate) fn scene_entities(app: &crate::editor::app::EditorApp) -> Vec<SceneEntity> {
    let mut out = Vec::new();
    // En contexto entidad no hay `scene_id`: se usan las entidades de la escena
    // activa, igual que `resolveSceneEntitiesForVisualScript` en React.
    let wanted_scene = app
        .visual_script
        .scene_id
        .or(app.project_state.active_scene_id);
    for scene in &app.project_state.scenes {
        if wanted_scene.is_some_and(|id| scene.get("id").and_then(Value::as_u64) == Some(id)) {
            if let Some(entities) = scene.get("entities").and_then(Value::as_array) {
                for entity in entities {
                    out.push(entity_from_value(entity, app));
                }
            }
            if let Some(player) = scene.get("player") {
                if !player.is_null() {
                    out.push(entity_from_value(player, app));
                }
            }
            break;
        }
    }
    // El snapshot de React puede no traer `entities` en escenas inactivas:
    // completar con los metadatos vivos del motor.
    if out.is_empty() {
        for (key, meta) in &app.project_state.entity_meta {
            let Ok(id) = key.parse::<u64>() else {
                continue;
            };
            out.push(entity_from_meta(id, meta));
        }
    }
    out
}

fn entity_from_value(value: &Value, app: &crate::editor::app::EditorApp) -> SceneEntity {
    let id = value.get("id").and_then(Value::as_u64).unwrap_or(0);
    let meta = app.project_state.entity_meta_for(id);
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            meta.and_then(|m| m.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("Entity {id}"));
    let category = value
        .get("category")
        .and_then(Value::as_str)
        .or_else(|| {
            meta.and_then(|m| m.get("entity3dCategory"))
                .and_then(Value::as_str)
        })
        .unwrap_or("object")
        .to_string();
    let blueprint_id = value
        .get("blueprint_id")
        .and_then(Value::as_str)
        .or_else(|| {
            meta.and_then(|m| m.get("blueprintId"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default()
        .to_string();
    let animations = animation_names(value)
        .or_else(|| {
            meta.and_then(|m| m.get("animations"))
                .and_then(animation_names)
        })
        .unwrap_or_default();
    SceneEntity {
        id,
        name,
        category,
        blueprint_id,
        animations,
    }
}

fn entity_from_meta(id: u64, meta: &Value) -> SceneEntity {
    SceneEntity {
        id,
        name: meta
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Entity")
            .to_string(),
        category: meta
            .get("entity3dCategory")
            .and_then(Value::as_str)
            .unwrap_or("object")
            .to_string(),
        blueprint_id: meta
            .get("blueprintId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        animations: meta
            .get("animations")
            .and_then(animation_names)
            .unwrap_or_default(),
    }
}

fn animation_names(value: &Value) -> Option<Vec<String>> {
    let list = value.as_array()?;
    let names: Vec<String> = list
        .iter()
        .filter_map(|a| a.get("name").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    Some(names)
}

pub(crate) fn animation_names_for(entity: Option<&SceneEntity>) -> Vec<String> {
    entity.map(|e| e.animations.clone()).unwrap_or_default()
}

/// Nombres de los blueprints del proyecto (para agrupar el entorno).
fn blueprint_names(app: &crate::editor::app::EditorApp) -> BTreeMap<String, String> {
    app.project_state
        .blueprints
        .iter()
        .filter_map(|bp| {
            let id = bp.get("id").and_then(Value::as_str)?;
            let name = bp
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(id)
                .to_string();
            Some((id.to_string(), name))
        })
        .collect()
}

/// Construye los acordeones del panel izquierdo, igual que React.
pub(crate) fn variable_groups(
    app: &crate::editor::app::EditorApp,
    locale: crate::editor::cli::Locale,
) -> Vec<VariableGroup> {
    use crate::editor::cli::Locale;
    let tr = |es: &'static str, en: &'static str| match locale {
        Locale::Es => es,
        Locale::En => en,
    };
    let entities = scene_entities(app);
    let mut groups = Vec::new();

    if app.visual_script.context == GraphContext::Entity {
        let mut transform = Vec::new();
        let mut other = Vec::new();
        if let Some(id) = app.visual_script.entity_id {
            let mut item = VariableItem::new(
                format!("{} (id {id})", tr("Esta entidad", "This entity")),
                format!("entity.id == {id}"),
            )
            .with_detail(app.visual_script.entity_name.clone());
            item.label = tr("Esta entidad", "This entity").to_string();
            other.push(item);
        }
        transform.push(VariableItem::new("entity.x", "entity.x"));
        transform.push(VariableItem::new("entity.y", "entity.y"));
        other.insert(0, VariableItem::new("entity.id", "entity.id"));
        transform.push(VariableItem::new("entity.scale_x", "entity.scale_x"));
        transform.push(VariableItem::new("entity.scale_y", "entity.scale_y"));

        let animations: Vec<VariableItem> = entities
            .iter()
            .filter(|e| app.visual_script.entity_id.is_none_or(|id| id == e.id))
            .flat_map(|e| {
                e.animations.iter().map(move |name| {
                    let mut item = VariableItem::new(name.clone(), name.clone());
                    item.animation = Some((e.id, name.clone()));
                    item.with_detail(format!("id {}", e.id))
                })
            })
            .collect();
        if !transform.is_empty() {
            groups.push(VariableGroup::new(
                "entity-var-transform",
                tr("Transformación", "Transform"),
                transform,
            ));
        }
        if !animations.is_empty() {
            groups.push(VariableGroup::new(
                "entity-var-animations",
                tr("Animaciones", "Animations"),
                animations,
            ));
        }
        if !other.is_empty() {
            groups.push(VariableGroup::new(
                "entity-var-other",
                tr("Otros", "Others"),
                other,
            ));
        }
        return groups;
    }

    // Contexto escena: lógica + entidades por categoría, con jugador aparte.
    let mut globals = Vec::new();
    if !entities.is_empty() {
        let ids: Vec<String> = entities.iter().map(|e| e.id.to_string()).collect();
        globals.push(
            VariableItem::new(
                tr("Todos los ids de entidad (lista)", "All entity ids (list)"),
                format!("[{}]", ids.join(", ")),
            )
            .with_detail(tr("Todos los ids de entidad", "All entity ids")),
        );
    }
    globals.push(
        VariableItem::new(tr("Delta time (segundos)", "Delta time (seconds)"), "dt")
            .with_detail("dt"),
    );
    groups.push(VariableGroup::new(
        "scene-globals",
        tr("Lógica de escena", "Scene logic"),
        globals,
    ));

    let bp_names = blueprint_names(app);
    let mut base_env = Vec::new();
    let mut standalone = Vec::new();
    let mut by_blueprint: BTreeMap<String, Vec<SceneEntity>> = BTreeMap::new();
    let mut player: Option<&SceneEntity> = None;
    let mut characters = Vec::new();
    let mut objects = Vec::new();

    for entity in &entities {
        match entity.category.as_str() {
            "sun" | "ground" => base_env.push(entity),
            "environment" => {
                if entity.blueprint_id.is_empty() {
                    standalone.push(entity);
                } else {
                    by_blueprint
                        .entry(entity.blueprint_id.clone())
                        .or_default()
                        .push(entity.clone());
                }
            }
            "player" if player.is_none() => player = Some(entity),
            "character" => characters.push(entity),
            "object" => objects.push(entity),
            _ => {}
        }
    }

    let rows = |list: &[&SceneEntity]| -> Vec<VariableItem> {
        list.iter()
            .map(|e| {
                VariableItem::new(e.name.clone(), e.id.to_string())
                    .with_detail(format!("id {}", e.id))
            })
            .collect()
    };

    let mut environment = VariableGroup::new(
        "entity-environment",
        tr("Entorno", "Environment"),
        Vec::new(),
    );
    if !base_env.is_empty() {
        environment.nested.push(VariableGroup::new(
            "env-base",
            tr("Entorno base", "Base environment"),
            rows(&base_env),
        ));
    }
    for (bp_id, instances) in &by_blueprint {
        let name = bp_names
            .get(bp_id)
            .cloned()
            .unwrap_or_else(|| instances[0].name.clone());
        let mut items = Vec::new();
        let mut group = VariableGroup::new(format!("env-bp-{bp_id}"), name.clone(), Vec::new());
        group.copy_id = instances.iter().min_by_key(|e| e.id).map(|e| e.id);
        for entity in instances {
            items.push(
                VariableItem::new(entity.name.clone(), entity.id.to_string())
                    .with_detail(format!("id {}", entity.id)),
            );
        }
        group.items = items;
        environment.nested.push(group);
    }
    if !standalone.is_empty() {
        environment.items.extend(rows(&standalone));
    }
    if !environment.items.is_empty() || !environment.nested.is_empty() {
        groups.push(environment);
    }

    if !characters.is_empty() {
        groups.push(VariableGroup::new(
            "entity-character",
            tr("Personajes", "Characters"),
            rows(&characters),
        ));
    }
    if let Some(player) = player {
        let mut group = VariableGroup::new("entity-player", tr("Jugador", "Player"), Vec::new());
        group.copy_id = Some(player.id);
        let animations: Vec<VariableItem> = player
            .animations
            .iter()
            .map(|name| {
                let mut item = VariableItem::new(name.clone(), name.clone());
                item.animation = Some((player.id, name.clone()));
                item.with_detail(format!("id {}", player.id))
            })
            .collect();
        if animations.is_empty() {
            group.items.push(VariableItem::new(
                tr("Sin animaciones en la escena", "No animations in scene"),
                "",
            ));
            group.items[0].copy = None;
        } else {
            group.nested.push(VariableGroup::new(
                "player-animations",
                tr("Animaciones", "Animations"),
                animations,
            ));
        }
        groups.push(group);
    }
    if !objects.is_empty() {
        groups.push(VariableGroup::new(
            "entity-object",
            tr("Objetos", "Objects"),
            rows(&objects),
        ));
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_document_roundtrip() {
        let mut doc = GraphDocument::empty(GraphContext::Scene, Some(3), None);
        let event = doc.graph.insert_node(
            egui::pos2(10.0, 20.0),
            VisualNode::new(NodeKind::SceneBegin, "n_event".into(), NodeData::default()),
        );
        let mut data = NodeKind::Log.default_data();
        data.message = "Hola".into();
        let log = doc.graph.insert_node(
            egui::pos2(200.0, 20.0),
            VisualNode::new(NodeKind::Log, "n_log".into(), data),
        );
        doc.graph.connect(
            OutPinId {
                node: event,
                output: 0,
            },
            InPinId {
                node: log,
                input: 0,
            },
        );

        let json = doc.to_json();
        assert_eq!(json["version"], 1);
        assert_eq!(json["context"], "scene");
        assert_eq!(json["sceneId"], 3);
        assert_eq!(json["nodes"][0]["id"], "n_event");
        assert_eq!(json["nodes"][0]["type"], "event.scene_begin");
        assert_eq!(json["nodes"][1]["data"]["message"], "Hola");
        assert_eq!(json["edges"][0]["sourceHandle"], "exec");
        assert_eq!(json["edges"][0]["targetHandle"], "exec_in");

        let restored = GraphDocument::from_json(Some(&json), GraphContext::Scene, Some(3), None);
        assert_eq!(restored.graph.nodes().count(), 2);
        assert_eq!(restored.graph.wires().count(), 1);
        assert!(restored.node_by_uid("n_log").is_some());
    }

    #[test]
    fn node_catalog_matches_react() {
        let scene: Vec<&str> = NodeKind::ALL
            .into_iter()
            .filter(|kind| kind.available_in(GraphContext::Scene))
            .map(NodeKind::as_str)
            .collect();
        assert_eq!(
            scene,
            vec![
                "event.scene_begin",
                "event.tick",
                "flow.sequence",
                "flow.if",
                "flow.for_repeat",
                "flow.for_each_entity",
                "action.log",
                "action.play_animation",
                "action.set_reflection_tier",
            ]
        );
        let entity: Vec<&str> = NodeKind::ALL
            .into_iter()
            .filter(|kind| kind.available_in(GraphContext::Entity))
            .map(NodeKind::as_str)
            .collect();
        assert_eq!(
            entity,
            vec![
                "event.entity_start",
                "event.entity_update",
                "flow.sequence",
                "flow.if",
                "flow.for_repeat",
                "action.log",
                "action.play_animation",
                "action.set_scale",
                "action.move_to",
                "action.translate",
            ]
        );
    }
}
