import { createContext, useCallback, useContext, useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import { useEngineSend } from '../hooks/useEngineSend'

export type Locale = 'en' | 'es'

interface LanguageContextValue {
  locale:    Locale
  setLocale: (locale: Locale) => void
  /** Aplica el idioma que ya eligió el editor, sin volver a enviarlo. */
  syncLocale: (locale: Locale) => void
  toggleLocale: () => void
}

const LanguageContext = createContext<LanguageContextValue | null>(null)

function localeFromUrl(): Locale {
  const value = new URLSearchParams(window.location.search).get('locale')
  return value === 'es' ? 'es' : 'en'
}

export function LanguageProvider({
  children,
  initialLocale = localeFromUrl(),
}: {
  children: ReactNode
  initialLocale?: Locale
}) {
  const { send2d } = useEngineSend()
  const [locale, setLocaleState] = useState<Locale>(initialLocale)

  useEffect(() => {
    setLocaleState(initialLocale)
  }, [initialLocale])

  const setLocale = useCallback((next: Locale) => {
    if (next === locale) return
    setLocaleState(next)
    try { send2d({ cmd: 'set_locale', locale: next }) } catch { /* engine no iniciado */ }
  }, [locale, send2d])

  const syncLocale = useCallback((next: Locale) => {
    setLocaleState(next)
  }, [])

  const toggleLocale = useCallback(() => {
    const next: Locale = locale === 'en' ? 'es' : 'en'
    setLocaleState(next)
    try { send2d({ cmd: 'set_locale', locale: next }) } catch { /* engine no iniciado */ }
  }, [locale, send2d])

  return (
    <LanguageContext.Provider value={{ locale, setLocale, syncLocale, toggleLocale }}>
      {children}
    </LanguageContext.Provider>
  )
}

export function useLanguage(): LanguageContextValue {
  const ctx = useContext(LanguageContext)
  if (!ctx) throw new Error('useLanguage must be used within a LanguageProvider')
  return ctx
}
