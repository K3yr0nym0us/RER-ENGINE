import type { PluginCatalogEntry } from '../../shared-types/plugins'

/**
 * Catálogo embebido (ligero). Los artefactos pesados se descargan on-demand.
 * RER-AI no ejecuta inferencia local: usa modelos free de OpenCode Console por
 * defecto y OpenCode Go (token opcional) para modelos de pago.
 */
export const PLUGIN_CATALOG: PluginCatalogEntry[] = [
  {
    id: 'local-ai-assistant',
    name: 'RER-AI (OpenCode)',
    description:
      'Assistant powered by OpenCode. Reads only the docs/ai editor guides and acts on the UI with dedicated tools. Free models by default; OpenCode Go token enables paid models.',
    version: '2.0.0',
    downloadSizeLabel: '~125 MB (OpenCode runtime)',
    kind: 'download',
    openCode: {
      downloadUrl:
        'https://opencode.ai/update/api/latest/cli/opencode/opencode-windows-x64.zip',
      archiveInnerPath: 'opencode.exe',
      executableName: 'opencode.exe',
      sizeBytes: 124_800_929,
    },
  },
  {
    id: 'world-gen',
    name: 'World Generator',
    description:
      'Generación procedural de mundos 3D: Marching Cubes con superficies suaves, montañas, lagos, ríos, cuevas y biomas configurables con semilla determinista. La DLL nativa se descarga desde GitHub Releases al instalar (se publica como release del repo del proyecto).',
    version: '1.0.0',
    // La lógica de pago (licencia) se enganchará aquí: validar antes de descargar.
    downloadSizeLabel: '~0.2 MB (rer_worldgen_plugin.zip)',
    kind: 'download',
    artifact: {
      archiveInnerPath: 'rer_worldgen.dll',
      executableName: 'rer_worldgen.dll',
      // URL del ASSET del release (no de la página): la página devolvería HTML
      // y el instalador lo rechazaría por tamaño.
      downloadUrl:
        'https://github.com/K3yr0nym0us/RER-ENGINE/releases/download/worldgen-v1.0.0/rer-worldgen-1.0.0-win-x64.zip',
      // Tamaño exacto del ZIP publicado (verificado).
      sizeBytes: 201_366,
      // SHA-256 esperado del ZIP; se valida durante la instalación si está presente.
      sha256: '9a9819609643995d5433db9e124f7556ba9bdf604648c26a7d7db20a26516249',
    },
  },
]

export function getPluginCatalogEntry(id: string): PluginCatalogEntry | undefined {
  return PLUGIN_CATALOG.find((entry) => entry.id === id)
}
