import type { ProjectType, GameStyle } from '../../../../shared-types/types'

interface StyleOption {
  type:        GameStyle
  label:       string
  icon:        string
  description: string
  badge:       string
  color:       string
  available:   boolean
}

const OPTIONS_3D: StyleOption[] = [
  {
    type:        'first-person',
    label:       'Primera Persona',
    icon:        '◉',
    description: 'Cámara en los ojos del personaje. FPS, horror y simuladores de vuelo.',
    badge:       '1ª P',
    color:       '#f87171',
    available:   true,
  },
  {
    type:        'second-person',
    label:       'Segunda Persona',
    icon:        '◑',
    description: 'Cámara sobre el hombro. Shooters en tercera persona cercana y acción-aventura.',
    badge:       '2ª P',
    color:       '#fb923c',
    available:   false,
  },
  {
    type:        'third-person',
    label:       'Tercera Persona',
    icon:        '◎',
    description: 'Cámara detrás del personaje. RPG, aventuras y juegos de acción estándar.',
    badge:       '3ª P',
    color:       '#34d399',
    available:   false,
  },
  {
    type:        'top-down',
    label:       'Vista Cenital',
    icon:        '⊕',
    description: 'Cámara desde arriba. RTS, roguelikes, dungeon crawlers y bullet-hells.',
    badge:       'TOP',
    color:       '#38bdf8',
    available:   false,
  },
  {
    type:        'side-scroller',
    label:       'Vista Lateral',
    icon:        '⊢',
    description: 'Cámara de costado. Plataformas 3D, beat-em-ups y metroidvanias.',
    badge:       'SIDE',
    color:       '#facc15',
    available:   false,
  },
  {
    type:        'isometric',
    label:       'Isométrico',
    icon:        '◇',
    description: 'Perspectiva diagonal fija. RPG clásicos, estrategia y city-builders.',
    badge:       'ISO',
    color:       '#a78bfa',
    available:   false,
  },
]

const OPTIONS_BY_TYPE: Partial<Record<ProjectType, StyleOption[]>> = {
  '3D':   OPTIONS_3D,
  '2D':   [],
  '2.5D': [],
}

interface Props {
  projectType: ProjectType
  onSelect:    (style: GameStyle) => void
  onBack:      () => void
}

export function GameStyleSelector({ projectType, onSelect, onBack }: Props) {
  const options = OPTIONS_BY_TYPE[projectType] ?? []

  const typeBadgeColor =
    projectType === '3D'   ? '#34d399' :
    projectType === '2.5D' ? '#a78bfa' : '#38bdf8'

  return (
    <div className="style-selector-page">
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
            Tipo de proyecto
          </span>
          <span className="breadcrumb-sep">›</span>
          <span
            className="engine-type-badge"
            style={{ color: typeBadgeColor, background: `${typeBadgeColor}18`, border: `1px solid ${typeBadgeColor}40` }}
          >
            {projectType}
          </span>
          <span className="breadcrumb-sep">›</span>
          <span className="breadcrumb-current">Estilo de juego</span>
        </div>

        <div className="mt-3 selector-subtitle">
          SELECCIONA EL ESTILO DE JUEGO
        </div>
      </div>

      {/* Grid de tarjetas — 3 columnas */}
      <div className="style-cards-grid">
        {options.map((opt) => (
          <div key={opt.type} className="style-card-wrapper">
            <button
              onClick={() => opt.available && onSelect(opt.type)}
              disabled={!opt.available}
              className={`style-card${!opt.available ? ' style-card--disabled' : ''}`}
              onMouseEnter={(e) => {
                if (!opt.available) return
                const el = e.currentTarget
                el.style.borderColor = opt.color
                el.style.boxShadow   = `0 0 20px ${opt.color}30`
                el.style.transform   = 'translateY(-2px)'
              }}
              onMouseLeave={(e) => {
                if (!opt.available) return
                const el = e.currentTarget
                el.style.borderColor = '#2c3152'
                el.style.boxShadow   = 'none'
                el.style.transform   = 'translateY(0)'
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
                    PRÓXIMAMENTE
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
