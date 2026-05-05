import { useEffect, useState } from 'react';

import { Accordion } from 'react-bootstrap';
import { CircleSquare, Check2Square, Files, Pencil, Trash } from 'react-bootstrap-icons';

import AppTooltip from '../../../../../components/AppTooltip';
import { TransformPanel, AnimationsPanel, ScriptingPanel } from '.';

import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import type { BluePrintCategory, BluePrintEntry } from '@shared-types';

export function PropertiesAccordion({ projectType }: { projectType?: string }) {
  const { 
    selectedEntity, 
    send, 
    scenarioEntities, 
    characterEntities,
    entityTransformsRef,
    entityMetaRef,
    removeScenario,
    duplicateScenario,
    removeCharacter,
    duplicateCharacter,
    removeCollider,
    removeExecutionArea,
    addBlueprint,
  } = useContextEngine()

  const { openModal, closeModal } = useModal();
  const [entityNameDraft, setEntityNameDraft] = useState('');
  const [isEditingEntityName, setIsEditingEntityName] = useState(false);

  // Intercepta set_transform para mantener entityTransformsRef sincronizado
  // sin depender del evento entity_selected (que solo llega cuando el usuario clica la entidad).
  const handleSend = (cmd: any) => {
    if (cmd.cmd === 'set_transform' && selectedEntity) {
      entityTransformsRef.current[selectedEntity.id] = {
        position: cmd.position ?? selectedEntity.position,
        rotation: cmd.rotation ?? selectedEntity.rotation,
        scale:    cmd.scale    ?? selectedEntity.scale,
      }
    }
    send(cmd)
  }
  
  const is2D = projectType === '2D'

  useEffect(() => {
    setEntityNameDraft(selectedEntity?.name ?? '');
    setIsEditingEntityName(false);
  }, [selectedEntity?.id, selectedEntity?.name]);

  // Derivar directamente desde el contexto para que sea reactivo a cambios
  // por scripts (PhysicsChanged) sin necesitar estado local intermedio.
  const physicsEnabled = selectedEntity?.physicsEnabled ?? false
  const physicsType    = selectedEntity?.physicsType || 'dynamic'

  if (!selectedEntity) {
    return <p className="text-secondary fst-italic small mb-0 px-1">Haz click en un objeto para verlo</p>
  }

  const isScenario = scenarioEntities.some((s: any) => s.id === selectedEntity?.id)
  const isCharacter = characterEntities.some((c: any) => c.id === selectedEntity?.id)
  const isCollider = selectedEntity ? entityMetaRef.current[selectedEntity.id]?.kind === 'collider' : false
  const isExecutionArea = selectedEntity ? entityMetaRef.current[selectedEntity.id]?.kind === 'execution_area' : false

  const handleConfirmModal = (onConfirm: () => void, action: 'eliminar' | 'duplicar') => {
    openModal({
      title: 'Confirmar acción',
      body: (
        <div className="text-center">
          <p>¿Estás seguro de que deseas {action} esta entidad?</p>
          <p className="text-danger">Esta acción no se puede deshacer.</p>
          <div className="d-flex justify-content-center gap-2 mt-4">
            <button
              className="btn btn-danger"
              onClick={() => {
                onConfirm()
                closeModal()
              }}
            >
              Sí, {action}
            </button>
            <button className="btn btn-secondary" onClick={closeModal}>
              Cancelar
            </button>
           </div>
         </div>
       )
     })
  }

  const handleDuplicate = () => {
    if (isScenario) {
      handleConfirmModal(() => duplicateScenario(selectedEntity.id), 'duplicar');
    } else if (isCharacter) {
      handleConfirmModal(() => duplicateCharacter(selectedEntity.id), 'duplicar');
    } else if (isCollider) {
      const points = entityMetaRef.current[selectedEntity.id]?.points;
      if (!points || points.length !== 4) return;
      const shiftedPoints = points.map(([x, y]) => [x + 0.5, y + 0.5]) as [[number, number], [number, number], [number, number], [number, number]];
      handleConfirmModal(() => send({ cmd: 'create_collider_from_points', points: shiftedPoints }), 'duplicar');
    } else if (isExecutionArea) {
      const points = entityMetaRef.current[selectedEntity.id]?.points;
      if (!points || points.length !== 4) return;
      const shiftedPoints = points.map(([x, y]) => [x + 0.5, y + 0.5]) as [[number, number], [number, number], [number, number], [number, number]];
      handleConfirmModal(() => send({ cmd: 'create_execution_area_from_points', points: shiftedPoints }), 'duplicar');
    }
  }

  const handleRemove = () => {
    if (isScenario) {
      handleConfirmModal(() => removeScenario(selectedEntity.id), 'eliminar');
    } else if (isCharacter) {
      handleConfirmModal(() => removeCharacter(selectedEntity.id), 'eliminar');
    } else if (isCollider) {
      handleConfirmModal(() => removeCollider(selectedEntity.id), 'eliminar');
    } else if (isExecutionArea) {
      handleConfirmModal(() => removeExecutionArea(selectedEntity.id), 'eliminar');
    }
  }
  const trimmedEntityName = entityNameDraft.trim();
  const hasValidEntityName = trimmedEntityName.length > 0;
  const canRename = !!selectedEntity && hasValidEntityName && trimmedEntityName !== selectedEntity.name;

  return (
    <div>
      <div className="mb-2">
        <p className="prop-label">Nombre</p>
        <div className="input-group input-group-sm mt-1">
          <input
            type="text"
            value={entityNameDraft}
            onChange={(e) => setEntityNameDraft(e.target.value)}
            className="form-control bg-dark text-info border-secondary prop-input"
            aria-label="Nombre de entidad"
            disabled={!isEditingEntityName}
          />
          {!isEditingEntityName ? (
            <AppTooltip content="Editar nombre" place="top">
              <button
                type="button"
                className="btn btn-outline-secondary"
                onClick={() => setIsEditingEntityName(true)}
              >
                <Pencil />
              </button>
            </AppTooltip>
          ) : (
            <AppTooltip content="Guardar cambios" place="top">
              <button
                type="button"
                className="btn btn-outline-info"
                disabled={!hasValidEntityName}
                onClick={() => {
                  if (!hasValidEntityName) return;
                  if (canRename) {
                    if (entityMetaRef.current[selectedEntity.id]) {
                      entityMetaRef.current[selectedEntity.id].name = trimmedEntityName;
                    }
                    send({ cmd: 'set_entity_name', id: selectedEntity.id, name: trimmedEntityName });
                  }
                  setIsEditingEntityName(false);
                }}
              >
                <Check2Square />
              </button>
            </AppTooltip>
          )}
        </div>
      </div>

      {!isCollider && !isExecutionArea && (
        isScenario ? (
          <div className="mb-2">
            <p className="prop-label">Colisión</p>
            <div className="d-flex align-items-center gap-2 mt-1">
              <input
                type="checkbox"
                id="scenario-collision"
                className="form-check-input"
                checked={physicsEnabled}
                onChange={(e) => {
                  const next = e.target.checked
                  send({ cmd: 'set_physics', id: selectedEntity.id, enabled: next, body_type: 'static' })
                }}
              />
              <label htmlFor="scenario-collision" className="form-check-label text-light small mb-0">
                Con colisión
              </label>
            </div>
          </div>
        ) : (
          <div className="mb-2">
            <p className="prop-label">Física</p>
            <div className="d-flex align-items-center gap-2 mt-1">
              <input
                type="checkbox"
                id="physics-enabled"
                className="form-check-input"
                checked={physicsEnabled}
                onChange={(e) => {
                  const next = e.target.checked
                  send({ cmd: 'set_physics', id: selectedEntity.id, enabled: next, body_type: physicsType })
                }}
              />
              <label htmlFor="physics-enabled" className="form-check-label text-light small mb-0">
                Activar física
              </label>
            </div>
            {physicsEnabled && (
              <select
                value={physicsType}
                className="form-select form-select-sm bg-dark text-light border-secondary mt-1"
                onChange={(e) => {
                  const next = e.target.value
                  send({ cmd: 'set_physics', id: selectedEntity.id, enabled: true, body_type: next })
                }}
              >
                <option value="dynamic">Dinámico (gravedad)</option>
                <option value="static">Estático (no se mueve)</option>
                {!is2D && <option value="kinematic">Cinemático (por código)</option>}
              </select>
            )}
          </div>
        )
      )}

      {!isCollider && !isExecutionArea && (
        <Accordion className="prop-accordion">
          <Accordion.Item eventKey="transform">
            <Accordion.Header>Transformaciones</Accordion.Header>
            <Accordion.Body className="py-2 px-2">
              <TransformPanel entity={selectedEntity} is2D={is2D} onSend={handleSend} />
            </Accordion.Body>
          </Accordion.Item>

          <AnimationsPanel />

          <ScriptingPanel />
        </Accordion>
      )}

      {isExecutionArea && (
        <Accordion className="prop-accordion">
          <ScriptingPanel />
        </Accordion>
      )}

      <div className="mt-3 pt-2 border-top border-secondary">
        <button
          className="btn btn-sm btn-outline-primary w-100 mb-2"
          onClick={() => {
            const meta = entityMetaRef.current[selectedEntity.id];
            const kind = meta?.kind ?? 'model';
            const category: BluePrintCategory =
              kind === 'character' ? 'personaje' :
              kind === 'scenario'  ? 'entorno'   : 'objetos';

            const handleConfirm = () => {
              const transform = entityTransformsRef.current[selectedEntity.id];
              const entry: BluePrintEntry = {
                id:               `bp_${Date.now()}_${Math.random().toString(36).slice(2, 7)}`,
                name:             selectedEntity.name,
                category,
                kind,
                path:             meta?.path ?? '',
                scale:            transform?.scale ?? [1, 1, 1],
                physics_enabled:  meta?.physicsEnabled,
                physics_type:     meta?.physicsType,
                animations:       meta?.animations,
                scripts:          meta?.scripts,
                control_bindings: meta?.controlBindings,
              };
              addBlueprint(entry);
              closeModal();
            };

            openModal({
              title: 'Crear Blueprint',
              body: (
                <div className="text-center">
                  <CircleSquare size={40} className="text-primary mb-3" />
                  <p>Se creará un <strong>Blueprint</strong> basado en la entidad <strong>{selectedEntity.name}</strong>.</p>
                  <p className="text-secondary small">El Blueprint guardará toda la configuración actual de la entidad: transformaciones, física, animaciones y scripts.</p>
                  <p className="text-secondary small">Se guardará en la categoría <strong>{category}</strong>.</p>
                  <div className="d-flex justify-content-center gap-2 mt-4">
                    <button className="btn btn-primary" onClick={handleConfirm}>
                      Confirmar
                    </button>
                    <button className="btn btn-secondary" onClick={closeModal}>
                      Cancelar
                    </button>
                  </div>
                </div>
              ),
            });
          }}
        >
          <CircleSquare className="me-2" />
          Crear Blueprint
        </button>
        <div className="d-flex gap-2">
          <button
            className="btn btn-sm btn-outline-secondary flex-fill"
            onClick={handleDuplicate}
          >
            <Files className="me-2" />
            Duplicar
          </button>
          <button
            className="btn btn-sm btn-outline-danger flex-fill"
            onClick={handleRemove}
          >
            <Trash className="me-2" />
            Eliminar
          </button>
        </div>
      </div>
    </div>
  )
}

export default PropertiesAccordion;