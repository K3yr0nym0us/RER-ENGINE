import { useState } from 'react'
import type { ProjectType } from './App'
import type { ProjectConfig } from '../../shared/types'

interface ProjectOption {
  type:        ProjectType
  label:       string
  icon:        string
  description: string
  badge:       string
  badgeColor:  string
  available:   boolean
}

const OPTIONS: ProjectOption[] = [
  {
    type:        '2D',
    label:       'Proyecto 2D',
    icon:        '▣',
    description: 'Sprites, tilemaps y física plana. Ideal para juegos plataformer, top-down o puzzle.',
    badge:       '2D',
    badgeColor:  '#38bdf8',
    available:   true,
  },
  {
    type:        '3D',
    label:       'Proyecto 3D',
    icon:        '⬡',
    description: 'Motor completo con meshes, luces, sombras y física 3D usando wgpu + Rapier.',
    badge:       '3D',
    badgeColor:  '#34d399',
    available:   false,
  },
]

// Estilos reutilizables para una tarjeta genérica
const cardBase: React.CSSProperties = {
  width:        220,
  background:   'rgba(14,16,30,0.95)',
  border:       '1px solid #2c3152',
  borderRadius: 12,
  padding:      '28px 20px 24px',
  cursor:       'pointer',
  transition:   'border-color 0.18s, box-shadow 0.18s, transform 0.14s',
  textAlign:    'center',
  color:        '#fff',
}

const separator = (
  <div style={{ width: 1, background: '#2c3152', borderRadius: 1, alignSelf: 'stretch', margin: '0 4px' }} />
)

interface Props {
  onSelect:        (type: ProjectType) => void
  onLoadProject:   (cfg: ProjectConfig) => void
}

export function ProjectSelector({ onSelect, onLoadProject }: Props) {
  const [loadError, setLoadError] = useState<string | null>(null)

  const handleLoadProject = async () => {
    setLoadError(null)
    const cfg = await window.electronAPI.openProjectDialog()
    if (cfg === null) return
    if (!cfg.type || !cfg.gameStyle) {
      setLoadError('El archivo seleccionado no es un proyecto RER válido.')
      return
    }
    onLoadProject(cfg)
  }

  const hoverOn = (color: string) => (e: React.MouseEvent<HTMLButtonElement>) => {
    const el = e.currentTarget
    el.style.borderColor = color
    el.style.boxShadow   = `0 0 24px ${color}33`
    el.style.transform   = 'translateY(-3px)'
  }
  const hoverOff = (e: React.MouseEvent<HTMLButtonElement>) => {
    const el = e.currentTarget
    el.style.borderColor = '#2c3152'
    el.style.boxShadow   = 'none'
    el.style.transform   = 'translateY(0)'
  }

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
        <div className="mt-2" style={{ fontSize: 14, color: '#6b7280', letterSpacing: '0.08em' }}>
          SELECCIONA EL TIPO DE PROYECTO
        </div>
      </div>

      <div className="d-flex gap-4 align-items-stretch">

        {/* ── 1. Abrir proyecto existente ──────────────────────────────── */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          <button
            onClick={handleLoadProject}
            style={{ ...cardBase, height: '100%' }}
            onMouseEnter={hoverOn('#c084fc')}
            onMouseLeave={hoverOff}
          >
            <div style={{ fontSize: 42, marginBottom: 10, color: '#c084fc', lineHeight: 1 }}>◫</div>
            <div style={{
              display: 'inline-block', fontSize: 11, fontWeight: 700, letterSpacing: '0.12em',
              padding: '2px 10px', borderRadius: 20, marginBottom: 10,
              background: '#c084fc22', color: '#c084fc', border: '1px solid #c084fc55',
            }}>
              ABRIR
            </div>
            <div style={{ fontSize: 15, fontWeight: 700, marginBottom: 8, color: '#e2e8f0' }}>
              Proyecto existente
            </div>
            <div style={{ fontSize: 12, color: '#6b7280', lineHeight: 1.5 }}>
              Carga un proyecto guardado anteriormente desde un archivo <em>project.json</em>.
            </div>
          </button>
          {loadError && (
            <div style={{ fontSize: 11, color: '#f87171', textAlign: 'center', maxWidth: 220 }}>
              {loadError}
            </div>
          )}
        </div>

        {separator}

        {/* ── 2 & 3. Tipos de proyecto (2D / 3D) ───────────────────────── */}
        {OPTIONS.map((opt) => (
          <div key={opt.type} style={{ position: 'relative' }}>
            <button
              onClick={() => opt.available && onSelect(opt.type)}
              disabled={!opt.available}
              style={{
                ...cardBase,
                height:   '100%',
                cursor:   opt.available ? 'pointer' : 'not-allowed',
                opacity:  opt.available ? 1 : 0.72,
              }}
              onMouseEnter={opt.available ? hoverOn(opt.badgeColor) : undefined}
              onMouseLeave={opt.available ? hoverOff : undefined}
            >
              <div style={{ fontSize: 42, marginBottom: 10, color: opt.badgeColor, lineHeight: 1 }}>
                {opt.icon}
              </div>
              <div style={{
                display: 'inline-block', fontSize: 11, fontWeight: 700, letterSpacing: '0.12em',
                padding: '2px 10px', borderRadius: 20, marginBottom: 10,
                background: `${opt.badgeColor}22`, color: opt.badgeColor, border: `1px solid ${opt.badgeColor}55`,
              }}>
                {opt.badge}
              </div>
              <div style={{ fontSize: 15, fontWeight: 700, marginBottom: 8, color: opt.available ? '#e2e8f0' : '#9ca3af' }}>
                {opt.label}
              </div>
              <div style={{ fontSize: 12, color: opt.available ? '#6b7280' : '#4b5563', lineHeight: 1.5 }}>
                {opt.description}
              </div>
              {!opt.available && (
                <div style={{ marginTop: 14 }}>
                  <span style={{
                    fontSize: 9, fontWeight: 700, letterSpacing: '0.1em',
                    padding: '2px 10px', borderRadius: 8,
                    background: `${opt.badgeColor}18`, color: `${opt.badgeColor}bb`, border: `1px solid ${opt.badgeColor}35`,
                  }}>
                    PRÓXIMAMENTE
                  </span>
                </div>
              )}
            </button>
          </div>
        ))}

        {separator}

        {/* ── 4. Proyecto desde cero ───────────────────────────────────── */}
        <button
          onClick={() => onSelect('scratch')}
          style={{ ...cardBase }}
          onMouseEnter={hoverOn('#fb923c')}
          onMouseLeave={hoverOff}
        >
          <div style={{ fontSize: 42, marginBottom: 10, color: '#fb923c', lineHeight: 1 }}>✦</div>
          <div style={{
            display: 'inline-block', fontSize: 11, fontWeight: 700, letterSpacing: '0.12em',
            padding: '2px 10px', borderRadius: 20, marginBottom: 10,
            background: '#fb923c22', color: '#fb923c', border: '1px solid #fb923c55',
          }}>
            NUEVO
          </div>
          <div style={{ fontSize: 15, fontWeight: 700, marginBottom: 8, color: '#e2e8f0' }}>
            Proyecto desde cero
          </div>
          <div style={{ fontSize: 12, color: '#6b7280', lineHeight: 1.5 }}>
            Abre el motor con una escena vacía y un cubo de referencia listo para editar.
          </div>
        </button>

      </div>

      {/* Footer */}
      <div className="mt-5" style={{ fontSize: 11, color: '#374151', letterSpacing: '0.06em' }}>
        React TS · Electron TS · Rust (wgpu)
      </div>
    </div>
  )
}
