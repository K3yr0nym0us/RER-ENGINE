import { Accordion } from 'react-bootstrap';
import { Controller, KeyboardFill } from 'react-bootstrap-icons';

import AppTooltip from '../../../../../components/AppTooltip';
import { useControlBindings } from '../../../../../hooks/useControlBindings';

export function ControlsAccordion() {
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
      <Accordion.Header>Controles</Accordion.Header>
      <Accordion.Body className="py-2 px-2">
        <div className="d-flex flex-column gap-2">
          <div>
            <label htmlFor="character-controls-select" className="form-label mb-1 small text-secondary">
              Personaje para asignar controles
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
              {!hasCharacters && <option value="">No hay personajes creados</option>}
              {characterOptions.map((option) => (
                <option key={option.id} value={option.id}>
                  {option.label}
                </option>
              ))}
            </select>
          </div>

          <AppTooltip content={!hasCharacters ? 'Crea un personaje primero' : null} place="top">
            <button
              type="button"
              className="btn btn-sm btn-outline-info w-100 d-flex align-items-center justify-content-between"
              disabled={!controlsEnabled}
              onClick={() => openBindingsModal('keyboard_mouse')}
            >
              <span>
                <KeyboardFill className="me-2" />
                Configurar teclado + mouse
              </span>
              <span className="badge bg-dark border border-secondary">{keyboardBindingsCount}</span>
            </button>
          </AppTooltip>

          <AppTooltip content={!hasCharacters ? 'Crea un personaje primero' : null} place="top">
            <button
              type="button"
              className="btn btn-sm btn-outline-warning w-100 d-flex align-items-center justify-content-between"
              disabled={!controlsEnabled}
              onClick={() => openBindingsModal('gamepad')}
            >
              <span>
                <Controller className="me-2" />
                Configurar mandos
              </span>
              <span className="badge bg-dark border border-secondary">{gamepadBindingsCount}</span>
            </button>
          </AppTooltip>

          <small className="text-secondary fst-italic">
            Puedes editar controles en cualquier momento. Su deteccion por el motor aplica unicamente en modo juego.
          </small>
        </div>
      </Accordion.Body>
    </Accordion.Item>
  )
}

export default ControlsAccordion
