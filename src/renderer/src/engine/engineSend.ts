import type {
  EngineCommand2D,
  EngineCommand3D,
  ProjectType,
} from '@shared-types'

/** Envía un comando solo válido para motor 2D. */
export function send2d(cmd: EngineCommand2D): void {
  window.engine2d.send(cmd)
}

/** Envía un comando solo válido para motor 3D. */
export function send3d(cmd: EngineCommand3D): void {
  window.engine3d.send(cmd)
}

/** Comandos compartidos o cuando el tipo de proyecto ya está resuelto. */
export function sendMotor(
  projectType: ProjectType | null | undefined,
  cmd: EngineCommand2D | EngineCommand3D,
): void {
  if (projectType === '3D') {
    send3d(cmd as EngineCommand3D)
  } else {
    send2d(cmd as EngineCommand2D)
  }
}

export function createEngineSend(projectType: ProjectType | undefined) {
  return projectType === '3D' ? send3d : send2d
}
