import { Accordion } from 'react-bootstrap';
import { Controller, KeyboardFill } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import { useControlBindings } from '@hooks';
import { useTraslate } from '@hooks';

export function ControlsAccordion() {
  const { t } = useTraslate();
  const {
    selectedCharacterId,
    setSelectedCharacterId,
    characterOptions,
    keyboardBindingsCount,
    gamepadBindingsCount,
    openBindingsModal,
  } = useControlBindings()

  const hasCharacters = characterOptions.length > 0
  const controlsEnabled = hasCharacters

  return (
    <Accordion.Item eventKey="controles">
      <Accordion.Header>{t('Controls')}</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <div className="d-flex flex-column gap-2">
          <div>
            <label htmlFor="character-controls-select" className="form-label mb-1 small text-secondary">
              {t('Character to assign controls')}
            </label>
            <select
              id="character-controls-select"
              className="form-select form-select-sm bg-dark text-light border-secondary"
              value={selectedCharacterId ?? ''}
              onChange={(event) => {
                const id = Number(event.target.value)
                setSelectedCharacterId(Number.isNaN(id) ? null : id)
              }}
              disabled={!hasCharacters}
            >
              {!hasCharacters && <option value="">{t('No characters created')}</option>}
              {characterOptions.map((option) => (
                <option key={option.id} value={option.id}>
                  {option.label}
                </option>
              ))}
            </select>
          </div>

          <AppTooltip content={!hasCharacters ? t('Create a character first') : null} place="top">
            <button
              type="button"
              className="btn btn-sm btn-outline-info w-100 d-flex align-items-center justify-content-between"
              disabled={!controlsEnabled}
              onClick={() => openBindingsModal('keyboard_mouse')}
            >
              <span>
                <KeyboardFill className="me-2" />
                {t('Configure keyboard + mouse')}
              </span>
              <span className="badge bg-dark border border-secondary">{keyboardBindingsCount}</span>
            </button>
          </AppTooltip>

          <AppTooltip content={!hasCharacters ? t('Create a character first') : null} place="top">
            <button
              type="button"
              className="btn btn-sm btn-outline-warning w-100 d-flex align-items-center justify-content-between"
              disabled={!controlsEnabled}
              onClick={() => openBindingsModal('gamepad')}
            >
              <span>
                <Controller className="me-2" />
                {t('Configure gamepads')}
              </span>
              <span className="badge bg-dark border border-secondary">{gamepadBindingsCount}</span>
            </button>
          </AppTooltip>

          <small className="text-secondary fst-italic">
            {t('You can edit controls at any time. Motor detection applies only in game mode.')}
          </small>
        </div>
      </Accordion.Body>
    </Accordion.Item>
  )
}

export default ControlsAccordion
