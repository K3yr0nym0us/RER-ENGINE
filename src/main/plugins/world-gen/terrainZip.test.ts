import { describe, expect, it } from 'vitest'

import { terrainZipRelPath } from './terrainZip'

describe('terrainZipRelPath', () => {
  it('acepta fotos en terrain/ en la raíz del ZIP', () => {
    expect(terrainZipRelPath('terrain/Grass.jpg')).toBe('terrain/Grass.jpg')
  })

  it('acepta barras de Windows y una carpeta intermedia', () => {
    expect(terrainZipRelPath('stage\\terrain\\Mud.jpg')).toBe('terrain/Mud.jpg')
  })

  it('rechaza rutas fuera de terrain o con traversal', () => {
    expect(terrainZipRelPath('rer_worldgen.dll')).toBeNull()
    expect(terrainZipRelPath('terrain/sub/Grass.jpg')).toBeNull()
    expect(terrainZipRelPath('terrain/../secret.jpg')).toBeNull()
  })
})
