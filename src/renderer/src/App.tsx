import type { EngineStartPayload, GameStyle, OpenProjectResult, ProjectType } from '@shared-types';
import { DEFAULT_3D_CAMERA_MODE } from '@shared-types';
import { useTraslate } from '@hooks';
import { LanguageToggleButton } from './components/LanguageToggleButton';
import { THEME_PRIMARY } from './styles/theme';
import imgLogo from './imgs/RER-ENGINE-LOGO.png';

interface ProjectOption {
  type: ProjectType
  label: string
  icon: string
  description: string
  badge: string
  badgeColor: string
}

function getOptions(t: (key: string) => string): ProjectOption[] {
  return [
    {
      type: '2D',
      label: t('2D Project'),
      icon: '▣',
      description: t('Sprites, tilemaps and flat physics. Ideal for platformer, top-down or puzzle games.'),
      badge: '2D',
      badgeColor: '#38bdf8',
    },
    {
      type: '3D',
      label: t('3D Project'),
      icon: '⬡',
      description: t('Full engine with meshes, lights, shadows and 3D physics using Vulkan + Rapier.'),
      badge: '3D',
      badgeColor: '#34d399',
    },
  ]
}

const separator = (
  <div style={{ width: 1, background: '#2c3152', borderRadius: 1, alignSelf: 'stretch', margin: '0 4px' }} />
)

/** Única pantalla React: elige proyecto y arranca el motor correspondiente. */
export default function App() {
  const { t } = useTraslate()
  const options = getOptions(t)

  const start = (projectType: ProjectType, gameStyle: GameStyle, savePath: string | false = false, extractDir?: string) => {
    const payload: EngineStartPayload = {
      projectType,
      mode: projectType === '2D' ? false : gameStyle,
      save_path: savePath,
      ...(extractDir ? { extract_dir: extractDir } : {}),
    }
    window.electronAPI.setGameStyle(payload)
  }

  const handleLoad = async () => {
    const result: OpenProjectResult | null = await window.electronAPI.openProjectDialog()
    if (!result) return
    start(result.project.type, result.project.gameStyle, result.filePath, result.extractDir)
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
            onClick={() => void handleLoad()}
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
        </div>

        {separator}

        {options.map((opt) => (
          <div key={opt.type} style={{ position: 'relative' }}>
            <button
              onClick={() => start(opt.type, opt.type === '2D' ? 'top-down' : DEFAULT_3D_CAMERA_MODE)}
              className="selector-card"
              style={{ height: '100%' }}
              onMouseEnter={hoverOn(opt.badgeColor)}
              onMouseLeave={hoverOff}
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
              <div style={{ fontSize: 15, fontWeight: 700, marginBottom: 8, color: '#e2e8f0' }}>
                {opt.label}
              </div>
              <div style={{ fontSize: 12, color: '#6b7280', lineHeight: 1.5 }}>
                {opt.description}
              </div>
            </button>
          </div>
        ))}
      </div>

      <div className="mt-5" style={{ fontSize: 12, color: '#374151', letterSpacing: '0.04em' }}>
        React · Electron · Rust
      </div>
    </div>
  )
}
