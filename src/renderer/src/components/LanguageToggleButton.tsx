import { useLanguage } from '@context';

interface LanguageToggleButtonProps {
  variant?: 'compact' | 'sidebar';
  className?: string;
}

export function LanguageToggleButton({ variant = 'compact', className = '' }: LanguageToggleButtonProps) {
  const { locale, toggleLocale } = useLanguage();

  if (variant === 'sidebar') {
    return (
      <button
        onClick={toggleLocale}
        title={locale === 'en' ? 'Switch to Spanish' : 'Cambiar a inglés'}
        style={{
          background:   '#0f1120',
          border:       '1px solid #2c3152',
          borderRadius: 6,
          color:        '#94a3b8',
          fontSize:     11,
          fontWeight:   700,
          letterSpacing: '0.06em',
          padding:      '4px 10px',
          cursor:       'pointer',
          transition:   'all 0.2s',
        }}
        onMouseEnter={(e) => {
          (e.currentTarget as HTMLButtonElement).style.color = '#cbd5e1';
          (e.currentTarget as HTMLButtonElement).style.borderColor = '#475569';
        }}
        onMouseLeave={(e) => {
          (e.currentTarget as HTMLButtonElement).style.color = '#94a3b8';
          (e.currentTarget as HTMLButtonElement).style.borderColor = '#2c3152';
        }}
        className={className}
      >
        {locale === 'en' ? 'ES' : 'EN'}
      </button>
    );
  }

  // variant === 'compact' (usado en TypeProjectSelector y GameStyleSelector)
  return (
    <button
      onClick={toggleLocale}
      title={locale === 'en' ? 'Switch to Spanish' : 'Cambiar a inglés'}
      style={{
        position:     'absolute',
        top:          16,
        right:        16,
        background:   '#0f1120',
        border:       '1px solid #2c3152',
        borderRadius: 6,
        color:        '#94a3b8',
        fontSize:     12,
        fontWeight:   700,
        letterSpacing: '0.06em',
        padding:      '5px 12px',
        cursor:       'pointer',
        transition:   'all 0.2s',
      }}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLButtonElement).style.color = '#cbd5e1';
        (e.currentTarget as HTMLButtonElement).style.borderColor = '#475569';
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLButtonElement).style.color = '#94a3b8';
        (e.currentTarget as HTMLButtonElement).style.borderColor = '#2c3152';
      }}
    >
      {locale === 'en' ? 'ES' : 'EN'}
    </button>
  );
}
