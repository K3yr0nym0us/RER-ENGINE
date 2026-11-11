import { describe, expect, it } from 'vitest'

import { centerRectInBounds, engineWindowRectToDip } from './modalElectronSizes'

describe('engineWindowRectToDip', () => {
  it('converts physical pixels and scale to DIP', () => {
    expect(
      engineWindowRectToDip({ x: 256, y: 256, width: 1618, height: 1047, scale: 1.25 }),
    ).toEqual({ x: 204.8, y: 204.8, width: 1294.4, height: 837.6 })
  })

  it('rejects incomplete or invalid rects', () => {
    expect(engineWindowRectToDip(null)).toBeNull()
    expect(engineWindowRectToDip({ x: 0, y: 0 })).toBeNull()
    expect(
      engineWindowRectToDip({ x: 0, y: 0, width: 100, height: 100, scale: 0 }),
    ).toBeNull()
  })
})

describe('centerRectInBounds', () => {
  const engineRect = { x: 204.8, y: 204.8, width: 1294.4, height: 837.6 }
  const workArea = { x: 0, y: 0, width: 1920, height: 1040 }

  it('centers the modal frame on the engine window', () => {
    expect(centerRectInBounds(engineRect, { width: 400, height: 200 }, workArea)).toEqual({
      x: 652,
      y: 524,
    })
  })

  it('clamps the result to the working area', () => {
    expect(
      centerRectInBounds(
        { x: -500, y: -500, width: 100, height: 100 },
        { width: 400, height: 200 },
        { x: 0, y: 0, width: 800, height: 600 },
      ),
    ).toEqual({ x: 0, y: 0 })
  })
})
