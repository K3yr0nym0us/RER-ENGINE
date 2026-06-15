// Herramientas de editor 3D (quick-build, plane tools) y grid compartido con IPC de mundo.

pub(crate) mod mesh;
pub use mesh::GridConfig;

use crate::config_3d::plane_tools::PlaneToolKind;

#[derive(Debug)]
pub(crate) enum ActiveTool {
    None,
    QuickBuildPlace {
        cursor_world: Option<[f32; 3]>,
    },
    PlacePlaneTool {
        kind: PlaneToolKind,
        size: [f32; 2],
        cursor_world: Option<[f32; 3]>,
        /// Rotación horizontal (rad) alrededor del eje Y; tecla E incrementa 90°.
        yaw: f32,
    },
}

impl Default for ActiveTool {
    fn default() -> Self {
        ActiveTool::None
    }
}

pub(crate) fn is_editor_placement_tool(tool: &ActiveTool) -> bool {
    match tool {
        ActiveTool::QuickBuildPlace { .. } => true,
        ActiveTool::PlacePlaneTool { .. } => true,
        _ => false,
    }
}

pub(crate) fn is_plane_tool_active(tool: &ActiveTool) -> bool {
    matches!(tool, ActiveTool::PlacePlaneTool { .. })
}
