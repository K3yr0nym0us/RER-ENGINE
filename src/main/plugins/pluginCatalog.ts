import type { PluginCatalogEntry } from '../../shared-types/plugins'

/** Catálogo embebido (ligero). El modelo y llama-server se descargan on-demand. */
export const PLUGIN_CATALOG: PluginCatalogEntry[] = [
  {
    id: 'local-ai-assistant',
    name: 'Local AI Assistant',
    description:
      'On-device assistant powered by Qwen3-1.7B. Guides you through the editor sidebar and docs. Optional download; not included in the base install.',
    version: '1.0.0',
    downloadSizeLabel: '~1.9 GB (model + llama.cpp runtime)',
    model: {
      repo: 'Qwen/Qwen3-1.7B-GGUF',
      repoUrl: 'https://huggingface.co/Qwen/Qwen3-1.7B-GGUF',
      filename: 'Qwen3-1.7B-Q8_0.gguf',
      downloadUrl:
        'https://huggingface.co/Qwen/Qwen3-1.7B-GGUF/resolve/main/Qwen3-1.7B-Q8_0.gguf',
      sizeBytes: 1_830_000_000,
      license: 'apache-2.0',
    },
    llamaServer: {
      downloadUrl:
        'https://github.com/ggml-org/llama.cpp/releases/download/b5212/llama-b5212-bin-win-avx2-x64.zip',
      archiveInnerPath: 'llama-server.exe',
      executableName: 'llama-server.exe',
      sizeBytes: 45_000_000,
    },
  },
]

export function getPluginCatalogEntry(id: string): PluginCatalogEntry | undefined {
  return PLUGIN_CATALOG.find((entry) => entry.id === id)
}
