import { ArrowLeft } from 'react-bootstrap-icons';

import { AppTooltip, LanguageToggleButton } from '@components';
import type { ProjectType, GameStyle } from '@shared-types';
import { useTraslate } from '@hooks';

interface StyleOption {
  type: GameStyle;
  label: string;
  icon: string;
  description: string;
  badge: string;
  color: string;
  available: boolean;
}

function getOptions3D(t: (key: string) => string): StyleOption[] {
  return [
    {
      type: 'first-person',
      label: t('First Person'),
      icon: '◉',
      description: t("Camera in the character's eyes. FPS, horror and flight simulators."),
      badge: '1ª P',
      color: '#f87171',
      available: true,
    },
    {
      type: 'second-person',
      label: t('Second Person'),
      icon: '◑',
      description: t('Over-the-shoulder camera. Close third-person shooters and action-adventure.'),
      badge: '2ª P',
      color: '#fb923c',
      available: false,
    },
    {
      type: 'third-person',
      label: t('Third Person'),
      icon: '◎',
      description: t('Camera behind the character. RPG, adventure and standard action games.'),
      badge: '3ª P',
      color: '#34d399',
      available: false,
    },
    {
      type: 'top-down',
      label: t('Top Down'),
      icon: '⊕',
      description: t('Camera from above. RTS, roguelikes, dungeon crawlers and bullet-hells.'),
      badge: 'TOP',
      color: '#38bdf8',
      available: false,
    },
    {
      type: 'side-scroller',
      label: t('Side Scroller'),
      icon: '⊢',
      description: t('Side camera. 3D platformers, beat-em-ups and metroidvanias.'),
      badge: 'SIDE',
      color: '#facc15',
      available: false,
    },
    {
      type: 'isometric',
      label: t('Isometric'),
      icon: '◇',
      description: t('Fixed diagonal perspective. Classic RPGs, strategy and city-builders.'),
      badge: 'ISO',
      color: '#a78bfa',
      available: false,
    },
  ]
}

function getOptionsByType(t: (key: string) => string): Partial<Record<ProjectType, StyleOption[]>> {
  return {
    '3D': getOptions3D(t),
    '2D': [],
  }
}

interface Props {
  projectType: ProjectType
  onSelect:    (style: GameStyle) => void
  onBack:      () => void
}

export function GameStyleSelector({ projectType, onSelect, onBack }: Props) {
  const { t } = useTraslate()

  const options = getOptionsByType(t)[projectType] ?? []

  const typeBadgeColor = projectType === '3D' ? '#34d399' : '#38bdf8';

  return (
    <div className="style-selector-page" style={{ position: 'relative' }}>
      {/* Botón toggle de idioma */}
      <LanguageToggleButton variant="compact" />

      {/* ── Botón volver ─────────────────────────────────────────────── */}
      <AppTooltip content={t('Back to project selector')} place="right">
        <button
          onClick={onBack}
          className="btn rounded-circle d-flex align-items-center justify-content-center position-fixed z-50 bg-dark border border-secondary"
          style={{ top: 24, left: 28, width: 56, height: 56 }}
          onMouseEnter={(e) => {
            Object.assign(e.currentTarget.style, { borderColor: '#c084fc', color: '#c084fc', boxShadow: '0 0 20px #c084fc44' })
          }}
          onMouseLeave={(e) => {
            Object.assign(e.currentTarget.style, { borderColor: '#2c3152', color: '#94a3b8', boxShadow: 'none' })
          }}
        >
          <ArrowLeft size={24} />
        </button>
      </AppTooltip>

      {/* Título */}
      <div className="mb-5 text-center">
        <div className="engine-logo">
          ⬡ RER-ENGINE
        </div>

        {/* Breadcrumb */}
        <div className="d-flex align-items-center justify-content-center gap-2 mt-3">
          <span
            role="button"
            tabIndex={0}
            onClick={onBack}
            onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') onBack() }}
            className="breadcrumb-back"
            onMouseEnter={(e) => (e.currentTarget.style.color = '#c084fc')}
            onMouseLeave={(e) => (e.currentTarget.style.color = '#4b5280')}
          >
            {t('Project type')}
          </span>
          <span className="breadcrumb-sep">›</span>
          <span
            className="engine-type-badge"
            style={{ color: typeBadgeColor, background: `${typeBadgeColor}18`, border: `1px solid ${typeBadgeColor}40` }}
          >
            {projectType}
          </span>
          <span className="breadcrumb-sep">›</span>
          <span className="breadcrumb-current">{t('Game style')}</span>
        </div>

        <div className="mt-3 selector-subtitle">
          {t('SELECT GAME STYLE')}
        </div>
      </div>

      {/* Grid de tarjetas — 3 columnas */}
      <div className="style-cards-grid">
        {options.map((opt) => (
          <div key={opt.type} className="style-card-wrapper">
            <button
              onClick={() => opt.available && onSelect(opt.type)}
              disabled={!opt.available}
              className={`p-3 style-card${!opt.available ? ' style-card--disabled' : ''}`}
              onMouseEnter={(e) => {
                if (!opt.available) return
                Object.assign(e.currentTarget.style, { borderColor: opt.color, boxShadow: `0 0 20px ${opt.color}30`, transform: 'translateY(-2px)' })
              }}
              onMouseLeave={(e) => {
                if (!opt.available) return
                Object.assign(e.currentTarget.style, { borderColor: '#2c3152', boxShadow: 'none', transform: 'translateY(0)' })
              }}
            >
              {/* Ícono */}
              <div className="style-card-icon" style={{ color: opt.color }}>
                {opt.icon}
              </div>

              {/* Badge */}
              <div
                className="style-badge"
                style={{ background: `${opt.color}20`, color: opt.color, border: `1px solid ${opt.color}45` }}
              >
                {opt.badge}
              </div>

              {/* Título */}
              <div className={`style-card-title${!opt.available ? ' style-card-title--disabled' : ''}`}>
                {opt.label}
              </div>

              {/* Descripción */}
              <div className={`style-card-desc${!opt.available ? ' style-card-desc--disabled' : ''}`}>
                {opt.description}
              </div>

              {/* Badge "Próximamente" */}
              {!opt.available && (
                <div className="coming-soon-wrapper">
                  <span
                    className="coming-soon-badge"
                    style={{ background: `${opt.color}18`, color: `${opt.color}bb`, border: `1px solid ${opt.color}35` }}
                  >
                    {t('COMING SOON')}
                  </span>
                </div>
              )}
            </button>
          </div>
        ))}
      </div>

      {/* Footer */}
      <div className="engine-footer">
        React TS · Electron TS · Rust (wgpu)
      </div>
    </div>
  )
}

export default GameStyleSelector;