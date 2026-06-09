import { useEffect } from 'react'

import { useLanguage } from '../context/LanguageContext'
import { usePlugins } from './usePlugins'

/** Muestra u oculta la ventana overlay del asistente según el estado del plugin. */
export function useAiAssistantOverlaySync(): void {
  const { locale } = useLanguage()
  const { llmStatus } = usePlugins()

  useEffect(() => {
    const shouldShow = llmStatus.enabled && llmStatus.installed
    if (shouldShow) {
      void window.electronAPI.pluginsStartLlm().then(() => {
        void window.electronAPI.aiAssistantShow({ locale })
      })
    } else {
      void window.electronAPI.aiAssistantHide()
      void window.electronAPI.pluginsStopLlm()
    }
  }, [llmStatus.enabled, llmStatus.installed, locale])
}
