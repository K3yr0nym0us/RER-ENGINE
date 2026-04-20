import type { ProjectType, GameStyle } from '../../shared/types'

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
    <div
      className="d-flex flex-column align-items-center justify-content-center"
      style={{ height: '100vh', background: '#050508', userSelect: 'none' }}
    >
      {/* Título */}
      <div className="mb-5 text-center">
        <div style={{ fontSize: 36, fontWeight: 800, color: '#c084fc', letterSpacing: '0.04em', lineHeight: 1 }}>
          ⬡ RER-ENGINE
        </div>

        {/* Breadcrumb */}
        <div className="d-flex align-items-center justify-content-center gap-2 mt-3">
          <span
            onClick={onBack}
            style={{
              fontSize: 12,
              color: '#4b5280',
              cursor: 'pointer',
              transition: 'color 0.15s',
            }}
            onMouseEnter={(e) => (e.currentTarget.style.color = '#c084fc')}
            onMouseLeave={(e) => (e.currentTarget.style.color = '#4b5280')}
          >
            Tipo de proyecto
          </span>
          <span style={{ color: '#2c3152', fontSize: 12 }}>›</span>
          <span
            style={{
              fontSize: 12,
              fontWeight: 700,
              letterSpacing: '0.06em',
              color: typeBadgeColor,
              background: `${typeBadgeColor}18`,
              border: `1px solid ${typeBadgeColor}40`,
              borderRadius: 8,
              padding: '1px 8px',
            }}
          >
            {projectType}
          </span>
          <span style={{ color: '#2c3152', fontSize: 12 }}>›</span>
          <span style={{ fontSize: 12, color: '#9ca3af' }}>Estilo de juego</span>
        </div>

        <div className="mt-3" style={{ fontSize: 14, color: '#6b7280', letterSpacing: '0.08em' }}>
          SELECCIONA EL ESTILO DE JUEGO
        </div>
      </div>

      {/* Grid de tarjetas — 3 columnas */}
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(3, 200px)',
          gap: 16,
        }}
      >
        {options.map((opt) => (
          <div key={opt.type} style={{ position: 'relative' }}>
            <button
              onClick={() => opt.available && onSelect(opt.type)}
              disabled={!opt.available}
              style={{
                width:        '100%',
                background:   'rgba(14,16,30,0.95)',
                border:       '1px solid #2c3152',
                borderRadius: 10,
                padding:      opt.available ? '22px 16px 18px' : '22px 16px 36px',
                cursor:       opt.available ? 'pointer' : 'not-allowed',
                transition:   'border-color 0.18s, box-shadow 0.18s, transform 0.14s',
                textAlign:    'center',
                color:        '#fff',
                opacity:      opt.available ? 1 : 0.72,
              }}
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
              <div style={{ fontSize: 34, marginBottom: 8, color: opt.color, lineHeight: 1 }}>
                {opt.icon}
              </div>

              {/* Badge */}
              <div
                style={{
                  display:       'inline-block',
                  fontSize:      10,
                  fontWeight:    700,
                  letterSpacing: '0.12em',
                  padding:       '1px 8px',
                  borderRadius:  16,
                  background:    `${opt.color}20`,
                  color:         opt.color,
                  border:        `1px solid ${opt.color}45`,
                  marginBottom:  8,
                }}
              >
                {opt.badge}
              </div>

              {/* Título */}
              <div style={{ fontSize: 13, fontWeight: 700, marginBottom: 6, color: opt.available ? '#e2e8f0' : '#9ca3af' }}>
                {opt.label}
              </div>

              {/* Descripción */}
              <div style={{ fontSize: 11, color: opt.available ? '#6b7280' : '#4b5563', lineHeight: 1.5 }}>
                {opt.description}
              </div>

              {/* Badge "Próximamente" */}
              {!opt.available && (
                <div style={{ marginTop: 12 }}>
                  <span
                    style={{
                      fontSize:      9,
                      fontWeight:    700,
                      letterSpacing: '0.1em',
                      padding:       '2px 10px',
                      borderRadius:  8,
                      background:    `${opt.color}18`,
                      color:         `${opt.color}bb`,
                      border:        `1px solid ${opt.color}35`,
                    }}
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
      <div className="mt-5" style={{ fontSize: 11, color: '#374151', letterSpacing: '0.06em' }}>
        React TS · Electron TS · Rust (wgpu)
      </div>
    </div>
  )
}
