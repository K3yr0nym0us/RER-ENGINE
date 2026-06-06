import { useState } from 'react';
import type { OpenProjectResult, ProjectType } from '@shared-types';
import { useTraslate } from '@hooks';
import { LanguageToggleButton } from '@components';
import { THEME_PRIMARY } from '../../styles/theme';

import imgLogo from '../../imgs/RER-ENGINE-LOGO.png';

interface ProjectOption {
  type:        ProjectType
  label:       string
  icon:        string
  description: string
  badge:       string
  badgeColor:  string
  available:   boolean
}

function getOptions(t: (key: string) => string): ProjectOption[] {
  return [
    {
      type:        '2D',
      label:       t('2D Project'),
      icon:        '▣',
      description: t('Sprites, tilemaps and flat physics. Ideal for platformer, top-down or puzzle games.'),
      badge:       '2D',
      badgeColor:  '#38bdf8',
      available:   true,
    },
    {
      type:        '3D',
      label:       t('3D Project'),
      icon:        '⬡',
      description: t('Full engine with meshes, lights, shadows and 3D physics using wgpu + Rapier.'),
      badge:       '3D',
      badgeColor:  '#34d399',
      available:   true,
    },
  ]
}

const separator = (
  <div style={{ width: 1, background: '#2c3152', borderRadius: 1, alignSelf: 'stretch', margin: '0 4px' }} />
)

interface Props {
  onSelect:      (type: ProjectType) => void
  onLoadProject: (result: OpenProjectResult) => void
}

export function TypeProjectSelector({ onSelect, onLoadProject }: Props) {
  const [loadError, setLoadError] = useState<string | null>(null)
  const { t } = useTraslate()

  const options = getOptions(t)

  const handleLoadProject = async () => {
    setLoadError(null)
    const result = await window.electronAPI.openProjectDialog()
    if (result === null) return
    if (!result.project.type || !result.project.gameStyle) {
      setLoadError(t('Invalid RER project file.'))
      return
    }
    onLoadProject(result)
  }

  const hoverOn = (color: string) => (e: React.MouseEvent<HTMLButtonElement>) => {
    Object.assign(e.currentTarget.style, {
      borderColor: color,
      boxShadow: `0 0 24px ${color}33`,
      transform: 'translateY(-3px)',
    })
  }
  const hoverOff = (e: React.MouseEvent<HTMLButtonElement>) => {
    Object.assign(e.currentTarget.style, {
      borderColor: '#2c3152',
      boxShadow: 'none',
      transform: 'translateY(0)',
    })
  }

  return (
    <div
      className="d-flex flex-column align-items-center justify-content-center"
      style={{ height: '100vh', background: '#050508', userSelect: 'none', position: 'relative' }}
    >
      <LanguageToggleButton variant="compact" />

      <div className="mb-3 text-center">
        <img src={imgLogo} alt="RER-ENGINE-LOGO" width={200} height={200} />
      </div>
      
      <div className="mt-2 mb-4 selector-subtitle fw-bold fs-4">
        {t('SELECT PROJECT TYPE')}
      </div>

      <div className="d-flex gap-4 align-items-stretch">

        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          <button
            onClick={handleLoadProject}
            className="selector-card"
            style={{ height: '100%' }}
            onMouseEnter={hoverOn(THEME_PRIMARY)}
            onMouseLeave={hoverOff}
          >
            <div className="selector-icon" style={{ color: THEME_PRIMARY }}>◫</div>
            <div
              className="selector-badge"
              style={{ background: `${THEME_PRIMARY}22`, color: THEME_PRIMARY, border: `1px solid ${THEME_PRIMARY}55` }}
            >
              {t('OPEN')}
            </div>
            <div style={{ fontSize: 15, fontWeight: 700, marginBottom: 8, color: '#e2e8f0' }}>
              {t('Existing project')}
            </div>
            <div style={{ fontSize: 12, color: '#6b7280', lineHeight: 1.5 }}>
              {t('Load a previously saved project from a .save file.')}
            </div>
          </button>
          {loadError && (
            <div style={{ fontSize: 12, color: '#f87171', textAlign: 'center', maxWidth: 220 }}>
              {loadError}
            </div>
          )}
        </div>

        {separator}

        {options.map((opt) => (
          <div key={opt.type} style={{ position: 'relative' }}>
            <button
              onClick={() => opt.available && onSelect(opt.type)}
              disabled={!opt.available}
              className={`selector-card${!opt.available ? ' selector-card--disabled' : ''}`}
              style={{ height: '100%' }}
              onMouseEnter={opt.available ? hoverOn(opt.badgeColor) : undefined}
              onMouseLeave={opt.available ? hoverOff : undefined}
            >
              <div className="selector-icon" style={{ color: opt.badgeColor }}>
                {opt.icon}
              </div>
              <div
                className="selector-badge"
                style={{ background: `${opt.badgeColor}22`, color: opt.badgeColor, border: `1px solid ${opt.badgeColor}55` }}
              >
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
                  <span
                    className="coming-soon-badge"
                    style={{ background: `${opt.badgeColor}18`, color: `${opt.badgeColor}bb`, border: `1px solid ${opt.badgeColor}35` }}
                  >
                    {t('COMING SOON')}
                  </span>
                </div>
              )}
            </button>
          </div>
        ))}
      </div>

      <div className="mt-5" style={{ fontSize: 12, color: '#374151', letterSpacing: '0.04em' }}>
        React TS · Electron TS · Rust (wgpu)
      </div>
    </div>
  )
}

export default TypeProjectSelector;
