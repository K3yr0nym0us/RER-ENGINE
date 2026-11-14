/**
 * Límites del generador circular. El motor (`rer_worldgen::scale`) vuelve a
 * clampear al generar: esto solo acota la UI para que no se pidan 50 montañas
 * en un disco pequeño.
 *
 * `Size` en la UI es el diámetro del disco en metros. El radio interno es la
 * mitad.
 */

/** Diámetro mínimo del disco (m). */
export const WORLDGEN_SIZE_MIN = 64
/** Diámetro máximo del disco (m). */
export const WORLDGEN_SIZE_MAX = 2000
/** Diámetro por defecto (m). */
export const WORLDGEN_SIZE_DEFAULT = 256

export const WORLDGEN_RADIUS_MIN = WORLDGEN_SIZE_MIN / 2
export const WORLDGEN_RADIUS_MAX = WORLDGEN_SIZE_MAX / 2
export const WORLDGEN_RADIUS_DEFAULT = WORLDGEN_SIZE_DEFAULT / 2

export function sizeToRadius(size: number): number {
  return size * 0.5
}

const SITE_EDGE = 24

export const BIOME_PRIORITY = ['grassland', 'forest', 'snow', 'desert', 'volcanic'] as const

export type WorldgenBiomeId = (typeof BIOME_PRIORITY)[number]

export function heightForRadius(radius: number): number {
  return Math.round(Math.min(256, Math.max(48, radius * 0.55)))
}

export function maxBiomeSlots(radius: number): number {
  if (radius < 80) return 2
  if (radius < 160) return 3
  if (radius < 280) return 4
  return 5
}

export function maxSitesInDisk(radius: number, spacing: number): number {
  const usable = Math.max(0, radius - SITE_EDGE)
  const space = Math.max(8, spacing)
  if (usable < space * 0.35) {
    return usable > 8 ? 1 : 0
  }
  const area = Math.PI * usable * usable
  const cell = Math.PI * (space * 0.5) ** 2
  return Math.max(0, Math.floor((area / cell) * 0.45))
}

export interface FeatureCaps {
  mountains: number
  hills: number
  valleys: number
  plains: number
  cliffs: number
  lakes: number
  riversMaxCount: number
  cavesMaxCount: number
  waterfallsMaxCount: number
}

export function featureCaps(radius: number): FeatureCaps {
  const r = radius
  const cap = (frac: number) => Math.max(1, maxSitesInDisk(r, r * frac))
  return {
    mountains: cap(0.62),
    hills: cap(0.28),
    valleys: cap(0.36),
    plains: cap(0.32),
    cliffs: cap(0.3),
    lakes: cap(0.38),
    riversMaxCount: cap(0.55),
    cavesMaxCount: Math.min(16, Math.max(1, Math.floor(r / 22))),
    waterfallsMaxCount: cap(0.7),
  }
}

export function featureSizes(radius: number) {
  const r = radius
  return {
    mountains: {
      minSize: Math.round(r * 0.10),
      maxSize: Math.round(r * 0.22),
      minSeparation: Math.round(r * 0.62),
      maxHeight: Math.round(Math.min(64, r * 0.22)),
      sharpness: 1.2,
    },
    hills: {
      minSize: Math.round(r * 0.08),
      maxSize: Math.round(r * 0.22),
      minSeparation: Math.round(r * 0.28),
      maxHeight: Math.round(Math.min(24, r * 0.12)),
    },
    valleys: {
      minSize: Math.round(r * 0.12),
      maxSize: Math.round(r * 0.3),
      minSeparation: Math.round(r * 0.36),
      maxDepth: Math.round(Math.min(30, r * 0.14)),
    },
    plains: {
      minSize: Math.round(r * 0.16),
      maxSize: Math.round(r * 0.4),
      minSeparation: Math.round(r * 0.32),
      maxRoughness: 1.5,
    },
    cliffs: {
      minSize: Math.round(r * 0.08),
      maxSize: Math.round(r * 0.22),
      minSeparation: Math.round(r * 0.3),
      dropHeight: 18,
    },
    lakes: {
      minSize: Math.round(r * 0.07),
      maxSize: Math.round(r * 0.2),
      minSeparation: Math.round(r * 0.38),
      maxDepth: 10,
    },
    rivers: {
      lengthMin: Math.round(Math.max(20, r * 0.45 * 0.45)),
      lengthMax: Math.round(Math.max(40, r * 1.5)),
      width: 4,
      depth: 3,
    },
  }
}

export function clampBiomes<T extends Record<WorldgenBiomeId, boolean>>(
  biomes: T,
  radius: number,
): T {
  const slots = maxBiomeSlots(radius)
  let kept = 0
  const next = { ...biomes }
  for (const id of BIOME_PRIORITY) {
    if (next[id]) {
      if (kept >= slots) {
        next[id] = false as T[WorldgenBiomeId]
      } else {
        kept += 1
      }
    }
  }
  if (!BIOME_PRIORITY.some((id) => next[id])) {
    next.grassland = true as T[WorldgenBiomeId]
  }
  return next
}

/** Porcentaje del mundo (0–100) por bioma. Los activos suman 100. */
export type BiomeShares = Record<WorldgenBiomeId, number>

function enabledBiomeIds(enabled: Record<WorldgenBiomeId, boolean>): WorldgenBiomeId[] {
  return BIOME_PRIORITY.filter((id) => enabled[id])
}

/** Reparte `total` en enteros proporcionales a `weights` (mayor resto). */
function distributeInteger(total: number, weights: number[]): number[] {
  const sum = weights.reduce((acc, weight) => acc + weight, 0)
  const safe = sum > 0 ? weights : weights.map(() => 1)
  const safeSum = sum > 0 ? sum : weights.length
  const exact = safe.map((weight) => (total * weight) / safeSum)
  const result = exact.map(Math.floor)
  let left = total - result.reduce((acc, value) => acc + value, 0)
  const order = exact
    .map((value, index) => ({ index, rest: value - Math.floor(value) }))
    .sort((a, b) => b.rest - a.rest)
  for (const { index } of order) {
    if (left <= 0) break
    result[index] += 1
    left -= 1
  }
  return result
}

function sharesFrom(ids: WorldgenBiomeId[], values: number[]): BiomeShares {
  const shares: BiomeShares = { grassland: 0, forest: 0, snow: 0, desert: 0, volcanic: 0 }
  ids.forEach((id, index) => {
    shares[id] = values[index]
  })
  return shares
}

/** Porcentaje mínimo de un bioma activo (si hay más de uno). */
export const MIN_BIOME_SHARE = 10

/**
 * Reparte `total` entre los biomas dando a cada uno `MIN_BIOME_SHARE` y el
 * sobrante en proporción a lo que cada uno pedía por encima del mínimo.
 */
function distributeWithMinimum(total: number, wanted: number[]): number[] {
  const extra = distributeInteger(
    total - MIN_BIOME_SHARE * wanted.length,
    wanted.map((value) => Math.max(0, value - MIN_BIOME_SHARE)),
  )
  return extra.map((value) => value + MIN_BIOME_SHARE)
}

/** Rango permitido del porcentaje de un bioma con `enabledCount` activos. */
export function biomeShareRange(enabledCount: number): { min: number; max: number } {
  if (enabledCount <= 1) return { min: 100, max: 100 }
  return { min: MIN_BIOME_SHARE, max: 100 - MIN_BIOME_SHARE * (enabledCount - 1) }
}

/**
 * Deja los biomas activos sumando 100 (en proporción, cada uno con al menos
 * `MIN_BIOME_SHARE`) y los inactivos a 0.
 */
export function normalizeBiomeShares(
  shares: BiomeShares,
  enabled: Record<WorldgenBiomeId, boolean>,
): BiomeShares {
  const ids = enabledBiomeIds(enabled)
  return sharesFrom(ids, distributeWithMinimum(100, ids.map((id) => shares[id])))
}

/**
 * Fija el porcentaje de `changed` (dentro de `biomeShareRange`) y reparte el
 * resto entre los demás activos en proporción a su valor actual, para que la
 * suma siga siendo 100 y ninguno baje de `MIN_BIOME_SHARE`.
 */
export function rebalanceBiomeShares(
  shares: BiomeShares,
  enabled: Record<WorldgenBiomeId, boolean>,
  changed: WorldgenBiomeId,
  value: number,
): BiomeShares {
  const others = enabledBiomeIds(enabled).filter((id) => id !== changed)
  if (!enabled[changed]) {
    return normalizeBiomeShares(shares, enabled)
  }
  if (others.length === 0) {
    return sharesFrom([changed], [100])
  }
  const range = biomeShareRange(others.length + 1)
  const fixed = Math.min(range.max, Math.max(range.min, Math.round(value)))
  const rest = distributeWithMinimum(
    100 - fixed,
    others.map((id) => shares[id]),
  )
  return sharesFrom([changed, ...others], [fixed, ...rest])
}

/** Al activar un bioma recibe una parte igual; al desactivarlo su parte se reparte. */
export function toggleBiomeShare(
  shares: BiomeShares,
  enabled: Record<WorldgenBiomeId, boolean>,
  toggled: WorldgenBiomeId,
): BiomeShares {
  if (!enabled[toggled]) {
    return normalizeBiomeShares({ ...shares, [toggled]: 0 }, enabled)
  }
  const count = enabledBiomeIds(enabled).length
  return rebalanceBiomeShares(shares, enabled, toggled, 100 / count)
}
