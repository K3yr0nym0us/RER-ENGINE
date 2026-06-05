/** Modales de código / nodos: al menos esta fracción de la pantalla disponible. */
export const MODAL_TALL_CONTENT_HEIGHT_RATIO = 0.5

export const MODAL_TALL_CONTENT_MIN_PX = 420

export const MODAL_TALL_COMPONENT_KEYS = new Set([
  'SceneScriptEditorModalBody',
  'ScriptEditorModalBody',
  'VisualScriptingModalBody',
])

/** Ventanas que el usuario puede redimensionar manualmente. */
export const MODAL_RESIZABLE_COMPONENT_KEYS = new Set(['VisualScriptingModalBody'])

export function screenAvailHeightPx(): number {
  return window.screen?.availHeight ?? 900
}

export function modalTallContentHeightPx(
  ratio = MODAL_TALL_CONTENT_HEIGHT_RATIO,
): number {
  return Math.max(MODAL_TALL_CONTENT_MIN_PX, Math.floor(screenAvailHeightPx() * ratio))
}

export function isTallModalComponent(componentKey: string | undefined): boolean {
  return componentKey != null && MODAL_TALL_COMPONENT_KEYS.has(componentKey)
}

export function isResizableModalComponent(componentKey: string | undefined): boolean {
  return componentKey != null && MODAL_RESIZABLE_COMPONENT_KEYS.has(componentKey)
}
