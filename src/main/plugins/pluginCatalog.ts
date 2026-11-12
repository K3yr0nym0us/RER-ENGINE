import type { PluginCatalogEntry } from '../../shared-types/plugins'

/** Catálogo embebido (ligero). Los artefactos pesados se descargan on-demand. */
export const PLUGIN_CATALOG: PluginCatalogEntry[] = [
  {
    id: 'local-ai-assistant',
    name: 'Local AI Assistant',
    description:
      'On-device assistant powered by Qwen3-1.7B. Guides you through the editor sidebar and docs. Optional download; not included in the base install.',
    version: '1.0.0',
    downloadSizeLabel: '~1.9 GB (model + llama.cpp runtime)',
    kind: 'download',
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
  {
    id: 'world-gen',
    name: 'World Generator',
    description:
      'Generación procedural de mundos 3D: Marching Cubes con superficies suaves, montañas, lagos, ríos, cuevas y biomas configurables con semilla determinista. La DLL nativa se descarga desde GitHub Releases al instalar (se publica como release del repo del proyecto).',
    version: '1.0.0',
    // La lógica de pago (licencia) se enganchará aquí: validar antes de descargar.
    downloadSizeLabel: '~0.3 MB (rer_worldgen_plugin.dll)',
    kind: 'download',
    artifact: {
      archiveInnerPath: 'rer_worldgen.dll',
      executableName: 'rer_worldgen.dll',
      // TODO(pago): reemplazar por la URL real del release al publicarlo.
      downloadUrl:
        'https://github.com/TsokasGit/rer-engine/releases/download/worldgen-v1.0.0/rer-worldgen-1.0.0-win-x64.zip',
      sizeBytes: 320_000,
      // SHA-256 esperado del ZIP; se valida durante la instalación si está presente.
      sha256: undefined,
    },
  },
]

export function getPluginCatalogEntry(id: string): PluginCatalogEntry | undefined {
  return PLUGIN_CATALOG.find((entry) => entry.id === id)
}
