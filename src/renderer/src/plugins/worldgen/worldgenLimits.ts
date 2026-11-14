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
