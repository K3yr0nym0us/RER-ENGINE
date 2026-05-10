import { createContext, useCallback, useContext, useState } from 'react'
import type { ReactNode } from 'react'

export type Locale = 'en' | 'es'

interface LanguageContextValue {
  locale:    Locale
  setLocale: (locale: Locale) => void
  toggleLocale: () => void
}

const LanguageContext = createContext<LanguageContextValue | null>(null)

export function LanguageProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>('en')

  const setLocale = useCallback((next: Locale) => {
    if (next === locale) return
    setLocaleState(next)
    try { window.engine.send({ cmd: 'set_locale', locale: next }) } catch { /* engine no iniciado */ }
  }, [locale])

  const toggleLocale = useCallback(() => {
    const next: Locale = locale === 'en' ? 'es' : 'en'
    setLocaleState(next)
    try { window.engine.send({ cmd: 'set_locale', locale: next }) } catch { /* engine no iniciado */ }
  }, [locale])

  return (
    <LanguageContext.Provider value={{ locale, setLocale, toggleLocale }}>
      {children}
    </LanguageContext.Provider>
  )
}

export function useLanguage(): LanguageContextValue {
  const ctx = useContext(LanguageContext)
  if (!ctx) throw new Error('useLanguage must be used within a LanguageProvider')
  return ctx
}
