export function getComponentKey(type: unknown): string {
  if (typeof type === 'string') return type
  if (typeof type === 'function') {
    const fn = type as { displayName?: string; name?: string }
    return fn.displayName || fn.name || 'Unknown'
  }
  return 'Unknown'
}

export { serializeModalProps, prepareModalElectronProps } from './modalElectronSerialize'
