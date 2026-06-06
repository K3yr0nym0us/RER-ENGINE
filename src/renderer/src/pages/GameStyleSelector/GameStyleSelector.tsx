import { ArrowLeft } from 'react-bootstrap-icons';

import { AppTooltip, LanguageToggleButton } from '@components';
import type { ProjectType, GameStyle, EngineStartPayload } from '@shared-types';
import { useTraslate } from '@hooks';

import { THEME_PRIMARY } from '../../styles/theme';

import imgLogo from '../../imgs/RER-ENGINE-LOGO.png';

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
  /** Ruta del `.save` si el flujo viene de «Abrir proyecto» antes de elegir estilo 3D. */
  savePath?:  string | null
  /** Carpeta extraída del `.save`; obligatoria si `savePath` está definido. */
  extractDir?: string | null
  onSelect:   (style: GameStyle) => void
  onBack:     () => void
}

export function GameStyleSelector({ projectType, savePath, extractDir, onSelect, onBack }: Props) {
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
            Object.assign(e.currentTarget.style, { borderColor: THEME_PRIMARY, color: THEME_PRIMARY, boxShadow: `0 0 20px ${THEME_PRIMARY}44` })
          }}
          onMouseLeave={(e) => {
            Object.assign(e.currentTarget.style, { borderColor: '#2c3152', color: '#94a3b8', boxShadow: 'none' })
          }}
        >
          <ArrowLeft size={24} />
        </button>
      </AppTooltip>

      {/* Título */}
      <div className="mb-4 text-center">
        <div className="engine-logo">
          <img src={imgLogo} alt="RER-ENGINE-LOGO" width={150} height={150} />
        </div>
        <div className="mt-3 selector-subtitle fw-bold fs-4 d-flex align-items-center justify-content-center">
          {t('SELECT GAME STYLE')}
          <span
            className="engine-type-badge ms-2"
            style={{ color: typeBadgeColor, background: `${typeBadgeColor}18`, border: `1px solid ${typeBadgeColor}40` }}
          >
            {projectType}
          </span>
        </div>
      </div>

      {/* Grid de tarjetas — 3 columnas */}
      <div className="style-cards-grid">
        {options.map((opt) => (
          <div key={opt.type} className="style-card-wrapper">
            <button
              onClick={() => {
                if (opt.available) {
                  const payload: EngineStartPayload = {
                    projectType,
                    mode: opt.type,
                    save_path: savePath ?? false,
                    ...(extractDir?.trim() ? { extract_dir: extractDir.trim() } : {}),
                  }
                  window.electronAPI.setGameStyle(payload)
                  onSelect(opt.type)
                }
              }}
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
      <div className="engine-footer mt-4">
        React TS · Electron TS · Rust (wgpu)
      </div>
    </div>
  )
}

export default GameStyleSelector;