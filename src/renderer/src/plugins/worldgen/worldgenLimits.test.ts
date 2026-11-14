import { describe, expect, it } from 'vitest'

import {
  WORLDGEN_RADIUS_DEFAULT,
  WORLDGEN_SIZE_DEFAULT,
  WORLDGEN_SIZE_MAX,
  clampBiomes,
  featureCaps,
  heightForRadius,
  maxBiomeSlots,
  maxSitesInDisk,
  sizeToRadius,
} from './worldgenLimits'

describe('worldgenLimits', () => {
  it('caps mountains on a small disk', () => {
    expect(featureCaps(64).mountains).toBeLessThanOrEqual(3)
    expect(maxSitesInDisk(64, 180)).toBeLessThanOrEqual(1)
  })

  it('grows biome slots with radius', () => {
    expect(maxBiomeSlots(50)).toBe(2)
    expect(maxBiomeSlots(WORLDGEN_RADIUS_DEFAULT)).toBe(3)
    expect(maxBiomeSlots(400)).toBe(5)
  })

  it('keeps grassland and drops volcanic on the default radius', () => {
    const biomes = clampBiomes(
      { grassland: true, forest: true, snow: true, desert: true, volcanic: true },
      WORLDGEN_RADIUS_DEFAULT,
    )
    expect(biomes.grassland).toBe(true)
    expect(biomes.forest).toBe(true)
    expect(biomes.snow).toBe(true)
    expect(biomes.desert).toBe(false)
    expect(biomes.volcanic).toBe(false)
  })

  it('derives height from radius', () => {
    expect(heightForRadius(128)).toBeGreaterThanOrEqual(48)
    expect(heightForRadius(128)).toBeLessThanOrEqual(256)
  })

  it('treats Size as diameter', () => {
    expect(WORLDGEN_SIZE_DEFAULT).toBe(256)
    expect(sizeToRadius(WORLDGEN_SIZE_DEFAULT)).toBe(WORLDGEN_RADIUS_DEFAULT)
    expect(sizeToRadius(1000)).toBe(500)
    expect(maxBiomeSlots(sizeToRadius(1000))).toBe(5)
    expect(sizeToRadius(WORLDGEN_SIZE_MAX)).toBe(1000)
  })
})
