import {
  DEFAULT_PLANE_HEIGHT,
  DEFAULT_PLANE_WIDTH,
  PLANE_TOOL_DEPTH,
  type PlaneToolName,
} from './PlaneToolContext'
import type { EngineCommand3D } from '@shared-types'

export function buildPlaneToolSetActiveCommand(tool: PlaneToolName | null): EngineCommand3D {
  if (!tool) {
    return { cmd: 'set_active_tool', tool: '' }
  }
  return {
    cmd: 'set_active_tool',
    tool,
    preview_scale: [DEFAULT_PLANE_WIDTH, DEFAULT_PLANE_HEIGHT, PLANE_TOOL_DEPTH],
  }
}
