import { useCallback } from 'react';
import { useLanguage } from '../context/LanguageContext';
import translations from '../locales/translations.json';

type TranslationKey = keyof typeof translations

export function useTraslate() {
  const { locale } = useLanguage()

  const t = useCallback(
    (key: string): string => {
      if (locale === 'en') return key
      return (translations as Record<string, string>)[key] ?? key
    },
    [locale],
  )

  return { t }
}
