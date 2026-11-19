import { describe, expect, it } from 'vitest'

import { isEngineStartPayload } from './isEngineStartPayload'

describe('isEngineStartPayload', () => {
  it('acepta proyecto nuevo 2D', () => {
    expect(isEngineStartPayload({ projectType: '2D', save_path: false })).toBe(true)
  })

  it('acepta proyecto nuevo 3D', () => {
    expect(isEngineStartPayload({ projectType: '3D', save_path: false })).toBe(true)
  })

  it('acepta apertura de .save sin tipo ni carpeta', () => {
    expect(isEngineStartPayload({ projectType: 'save', save_path: 'C:/juego.save' })).toBe(true)
  })

  it('rechaza payloads incompletos o desconocidos', () => {
    expect(isEngineStartPayload(null)).toBe(false)
    expect(isEngineStartPayload({ projectType: 'save' })).toBe(false)
    expect(isEngineStartPayload({ projectType: '4D', save_path: false })).toBe(false)
    expect(isEngineStartPayload({ save_path: false })).toBe(false)
  })
})
