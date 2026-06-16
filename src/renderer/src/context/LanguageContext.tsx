import { createContext, useCallback, useContext, useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import { send2d } from '../engine/engineSend'

export type Locale = 'en' | 'es'

interface LanguageContextValue {
  locale:    Locale
  setLocale: (locale: Locale) => void
  toggleLocale: () => void
}

const LanguageContext = createContext<LanguageContextValue | null>(null)

export function LanguageProvider({
  children,
  initialLocale = 'en',
}: {
  children: ReactNode
  initialLocale?: Locale
}) {
  const [locale, setLocaleState] = useState<Locale>(initialLocale)

  useEffect(() => {
    setLocaleState(initialLocale)
  }, [initialLocale])

  const setLocale = useCallback((next: Locale) => {
    if (next === locale) return
    setLocaleState(next)
    try { send2d({ cmd: 'set_locale', locale: next }) } catch { /* engine no iniciado */ }
  }, [locale])

  const toggleLocale = useCallback(() => {
    const next: Locale = locale === 'en' ? 'es' : 'en'
    setLocaleState(next)
    try { send2d({ cmd: 'set_locale', locale: next }) } catch { /* engine no iniciado */ }
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
