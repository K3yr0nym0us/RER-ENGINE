/** Relative dest path for a ZIP entry that is a terrain photo, or null. */
export function terrainZipRelPath(entryName: string): string | null {
  const parts = entryName
    .replace(/\\/g, '/')
    .split('/')
    .filter((part) => part.length > 0)
  const index = parts.lastIndexOf('terrain')
  if (index < 0 || index !== parts.length - 2) {
    return null
  }
  const file = parts[parts.length - 1]
  if (!file || file === '.' || file.includes('..')) {
    return null
  }
  return `terrain/${file}`
}
