import { describe, expect, it } from 'vitest'

import {
  type BiomeShares,
  MIN_BIOME_SHARE,
  WORLDGEN_RADIUS_DEFAULT,
  WORLDGEN_SIZE_DEFAULT,
  WORLDGEN_SIZE_MAX,
  biomeShareRange,
  clampBiomes,
  featureCaps,
  heightForRadius,
  maxBiomeSlots,
  maxSitesInDisk,
  normalizeBiomeShares,
  rebalanceBiomeShares,
  sizeToRadius,
  toggleBiomeShare,
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

describe('biome shares', () => {
  const enabled = { grassland: true, forest: true, snow: true, desert: false, volcanic: false }
  const sum = (shares: BiomeShares) => Object.values(shares).reduce((acc, v) => acc + v, 0)

  it('splits evenly among enabled biomes', () => {
    const shares = normalizeBiomeShares(
      { grassland: 0, forest: 0, snow: 0, desert: 0, volcanic: 0 },
      enabled,
    )
    expect(sum(shares)).toBe(100)
    expect(shares.desert).toBe(0)
    expect(shares.grassland).toBeGreaterThanOrEqual(33)
  })

  it('rebalances the others proportionally to keep 100', () => {
    const shares = rebalanceBiomeShares(
      { grassland: 50, forest: 30, snow: 20, desert: 0, volcanic: 0 },
      enabled,
      'grassland',
      80,
    )
    expect(shares).toEqual({ grassland: 80, forest: 10, snow: 10, desert: 0, volcanic: 0 })
  })

  it('splits the rest proportionally above the minimum', () => {
    const shares = rebalanceBiomeShares(
      { grassland: 40, forest: 40, snow: 20, desert: 0, volcanic: 0 },
      enabled,
      'grassland',
      50,
    )
    expect(shares).toEqual({ grassland: 50, forest: 33, snow: 17, desert: 0, volcanic: 0 })
  })

  it('keeps every enabled biome at or above the minimum share', () => {
    const shares = rebalanceBiomeShares(
      { grassland: 60, forest: 40, snow: 0, desert: 0, volcanic: 0 },
      enabled,
      'forest',
      130,
    )
    expect(shares.forest).toBe(100 - 2 * MIN_BIOME_SHARE)
    expect(shares.grassland).toBe(MIN_BIOME_SHARE)
    expect(shares.snow).toBe(MIN_BIOME_SHARE)
    expect(sum(shares)).toBe(100)

    const lowered = rebalanceBiomeShares(shares, enabled, 'snow', 2)
    expect(lowered.snow).toBe(MIN_BIOME_SHARE)
    expect(sum(lowered)).toBe(100)

    const normalized = normalizeBiomeShares(
      { grassland: 98, forest: 2, snow: 0, desert: 0, volcanic: 0 },
      enabled,
    )
    expect(Math.min(normalized.grassland, normalized.forest, normalized.snow)).toBe(
      MIN_BIOME_SHARE,
    )
    expect(sum(normalized)).toBe(100)
  })

  it('reports the allowed share range', () => {
    expect(biomeShareRange(1)).toEqual({ min: 100, max: 100 })
    expect(biomeShareRange(5)).toEqual({ min: 10, max: 60 })
  })

  it('pins a single enabled biome to 100', () => {
    const only = { grassland: false, forest: true, snow: false, desert: false, volcanic: false }
    const shares = rebalanceBiomeShares(
      { grassland: 0, forest: 40, snow: 0, desert: 0, volcanic: 0 },
      only,
      'forest',
      40,
    )
    expect(shares.forest).toBe(100)
  })

  it('gives a newly enabled biome an equal part and redistributes a removed one', () => {
    const added = toggleBiomeShare(
      { grassland: 50, forest: 50, snow: 0, desert: 0, volcanic: 0 },
      enabled,
      'snow',
    )
    expect(added.snow).toBe(33)
    expect(sum(added)).toBe(100)

    const removed = toggleBiomeShare(added, { ...enabled, snow: false }, 'snow')
    expect(removed.snow).toBe(0)
    expect(sum(removed)).toBe(100)
  })
})
