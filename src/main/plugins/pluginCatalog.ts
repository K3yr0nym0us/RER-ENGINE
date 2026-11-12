import type { PluginCatalogEntry } from '../../shared-types/plugins'

/**
 * Catálogo embebido (ligero). El runtime de OpenCode se descarga on-demand;
 * la inferencia no es local: usa modelos free de OpenCode Console por defecto
 * y OpenCode Go (token opcional) para modelos de pago.
 */
export const PLUGIN_CATALOG: PluginCatalogEntry[] = [
  {
    id: 'local-ai-assistant',
    name: 'RER-AI (OpenCode)',
    description:
      'Assistant powered by OpenCode. Reads only the docs/ai editor guides and acts on the UI with dedicated tools. Free models by default; OpenCode Go token enables paid models.',
    version: '2.0.0',
    downloadSizeLabel: '~125 MB (OpenCode runtime)',
    openCode: {
      downloadUrl:
        'https://opencode.ai/update/api/latest/cli/opencode/opencode-windows-x64.zip',
      archiveInnerPath: 'opencode.exe',
      executableName: 'opencode.exe',
      sizeBytes: 124_800_929,
    },
  },
]

export function getPluginCatalogEntry(id: string): PluginCatalogEntry | undefined {
  return PLUGIN_CATALOG.find((entry) => entry.id === id)
}
