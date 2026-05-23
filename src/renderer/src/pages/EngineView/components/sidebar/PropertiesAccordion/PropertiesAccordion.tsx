import { useEffect, useState } from 'react';

import { Accordion } from 'react-bootstrap';
import { CircleSquare, Check2Square, Pencil, Trash, Link45deg, ArrowsMove, BoxSeam } from 'react-bootstrap-icons';

import { AppTooltip } from '@components';
import { TransformPanel, AnimationsPanel, ScriptingPanel } from '.';
import { CreateEntityFromModelModalBody } from '../EntitiesAccordion/components/CreateEntityFromModelModalBody';

import { useContextEngine } from '@engine';
import { useModal } from '@modal';
import type { BluePrintCategory, BluePrintEntry } from '@shared-types';
import { isEditorCameraEntity, isEnvironmentEntity, isPlayerEntity } from '@shared-types';
import { useTraslate } from '@hooks';

export function PropertiesAccordion({ projectType }: { projectType?: string }) {
  const { t } = useTraslate();
  const { 
    selectedEntity, 
    send, 
    scenarioEntities, 
    characterEntities,
    entityTransformsRef,
    entityMetaRef,
    playerEntityIdRef,
    editorCameraEntityIdRef,
    models,
    replaceEntityModel,
    removeScenario,
    removeCharacter,
    removeEntity,
    removeCollider,
    removeExecutionArea,
    addBlueprint,
    multiSelectedIds,
    setEntityPhysics,
    updateEntityTransform,
    blueprints,
  } = useContextEngine()

  const { openModal, closeModal } = useModal();
  const [entityNameDraft, setEntityNameDraft] = useState('');
  const [isEditingEntityName, setIsEditingEntityName] = useState(false);

  const handleSend = (cmd: {
    cmd: string
    id?: number
    position?: [number, number, number]
    position_axis?: { axis: number; value: number }
    rotation?: [number, number, number, number]
    scale?: [number, number, number]
    scale_axis?: { axis: number; value: number }
    body_rotation_only?: boolean
    rotation_euler_delta?: { axis: number; degrees: number }
    rotation_euler_degrees?: [number, number, number]
  }) => {
    if (cmd.cmd === 'set_transform' && selectedEntity && cmd.id === selectedEntity.id) {
      updateEntityTransform(selectedEntity.id, {
        ...(cmd.position !== undefined ? { position: cmd.position } : {}),
        ...(cmd.position_axis !== undefined ? { positionAxis: cmd.position_axis } : {}),
        ...(cmd.rotation !== undefined ? { rotation: cmd.rotation } : {}),
        ...(cmd.scale !== undefined ? { scale: cmd.scale } : {}),
        ...(cmd.scale_axis !== undefined ? { scaleAxis: cmd.scale_axis } : {}),
        ...(cmd.body_rotation_only ? { bodyRotationOnly: true } : {}),
        ...(cmd.rotation_euler_delta !== undefined
          ? { rotationEulerDelta: cmd.rotation_euler_delta }
          : {}),
        ...(cmd.rotation_euler_degrees !== undefined
          ? { rotationEulerDegrees: cmd.rotation_euler_degrees }
          : {}),
      });
      return;
    }
    send(cmd);
  };
  
  const is2D = projectType === '2D'
  const isMultiSelect = multiSelectedIds.length > 1

  useEffect(() => {
    setEntityNameDraft(selectedEntity?.name ?? '');
    setIsEditingEntityName(false);
  }, [selectedEntity?.id, selectedEntity?.name]);

  // Derivar directamente desde el contexto para que sea reactivo a cambios
  // por scripts (PhysicsChanged) sin necesitar estado local intermedio.
  const physicsEnabled = selectedEntity?.physicsEnabled ?? false
  const physicsType    = selectedEntity?.physicsType || 'dynamic'

  if (!selectedEntity) {
    return <p className="text-secondary fst-italic small mb-0 px-1">{t('Click on an object to view it')}</p>
  }

  if (isMultiSelect) {
    const handleRemoveMultiple = () => {
      openModal({
        title: t('Confirm action'),
        body: (
          <div className="text-center">
            <p>{t('Are you sure you want to')} {t('delete')} <strong>{multiSelectedIds.length}</strong> {t('entities?')}</p>
            <p className="text-danger">{t('This action cannot be undone.')}</p>
            <div className="d-flex justify-content-center gap-2 mt-4">
              <button
                className="btn btn-danger"
                onClick={() => {
                  multiSelectedIds.forEach(id => {
                    if (isPlayerEntity(id, entityMetaRef.current[id], playerEntityIdRef.current)) return;
                    if (isEditorCameraEntity(id, entityMetaRef.current[id], editorCameraEntityIdRef.current)) return;
                    const kind = entityMetaRef.current[id]?.kind;
                    if (kind === 'scenario') removeScenario(id);
                    else if (kind === 'character') removeCharacter(id);
                    else if (kind === 'collider') removeCollider(id);
                    else if (kind === 'execution_area') removeExecutionArea(id);
                    else if (scenarioEntities.some((s: any) => s.id === id)) removeScenario(id);
                    else if (characterEntities.some((c: any) => c.id === id)) removeCharacter(id);
                    else removeEntity(id);
                  });
                  closeModal();
                }}
              >
                {t('Yes,')} {t('delete')}
              </button>
              <button className="btn btn-secondary" onClick={closeModal}>
                {t('Cancel')}
              </button>
            </div>
          </div>
        ),
      });
    };
    return (
      <div>
        <p className="text-secondary fst-italic small mb-0 px-1">
          {multiSelectedIds.length} {t('entities selected')}
        </p>
        <div className="mt-3 pt-2 border-top border-secondary">
          <button
            className="btn btn-sm btn-outline-danger w-100"
            onClick={handleRemoveMultiple}
          >
            <Trash className="me-2" />
            {t('Delete')} ({multiSelectedIds.length})
          </button>
        </div>
      </div>
    );
  }

  const isScenario = scenarioEntities.some((s: any) => s.id === selectedEntity?.id)
  const entityMeta = entityMetaRef.current[selectedEntity.id]
  const isEnvironment = isEnvironmentEntity(isScenario, entityMeta)
  const isPlayer = isPlayerEntity(selectedEntity.id, entityMeta, playerEntityIdRef.current)
  const isEditorCamera = isEditorCameraEntity(
    selectedEntity.id,
    entityMeta,
    editorCameraEntityIdRef.current,
  )
  const isCharacter = characterEntities.some((c: any) => c.id === selectedEntity?.id)
  const is3D = projectType === '3D'
  const hasEmbeddedModelClips =
    is3D &&
    (entityMeta?.animations?.some((a) => a.embedded_in_model) ?? false)
  const isCollider = selectedEntity ? entityMetaRef.current[selectedEntity.id]?.kind === 'collider' : false
  const isExecutionArea = selectedEntity ? entityMetaRef.current[selectedEntity.id]?.kind === 'execution_area' : false
  const isFromBlueprint = selectedEntity ? !!entityMetaRef.current[selectedEntity.id]?.blueprintId : false
  const linkedBlueprintName = isFromBlueprint
    ? (blueprints.find(bp => bp.id === entityMetaRef.current[selectedEntity!.id]?.blueprintId)?.name ?? null)
    : null

  const handleConfirmModal = (onConfirm: () => void, action: 'delete' | 'duplicate') => {
    openModal({
      title: t('Confirm action'),
      body: (
        <div className="text-center">
          <p>{t('Are you sure you want to')} {t(action)} {t('this entity?')}</p>
          <p className="text-danger">{t('This action cannot be undone.')}</p>
          <div className="d-flex justify-content-center gap-2 mt-4">
            <button
              className="btn btn-danger"
              onClick={() => {
                onConfirm()
                closeModal()
              }}
            >
              {t('Yes,')} {t(action)}
            </button>
            <button className="btn btn-secondary" onClick={closeModal}>
              {t('Cancel')}
            </button>
           </div>
         </div>
       )
     })
  }

  const handleRemove = () => {
    if (isPlayer) return;
    if (isScenario) {
      handleConfirmModal(() => removeScenario(selectedEntity.id), 'delete');
    } else if (isCharacter) {
      handleConfirmModal(() => removeCharacter(selectedEntity.id), 'delete');
    } else if (isCollider) {
      handleConfirmModal(() => removeCollider(selectedEntity.id), 'delete');
    } else if (isExecutionArea) {
      handleConfirmModal(() => removeExecutionArea(selectedEntity.id), 'delete');
    } else {
      handleConfirmModal(() => removeEntity(selectedEntity.id), 'delete');
    }
  }
  const trimmedEntityName = entityNameDraft.trim();
  const hasValidEntityName = trimmedEntityName.length > 0;
  const canRename = !!selectedEntity && hasValidEntityName && trimmedEntityName !== selectedEntity.name;

  return (
    <div>
      <div className="mb-2">
        <p className="prop-label">{t('Name')}</p>
        <div className="input-group input-group-sm mt-1">
          <input
            type="text"
            value={entityNameDraft}
            onChange={(e) => setEntityNameDraft(e.target.value)}
            className="form-control bg-dark text-info border-secondary prop-input"
            aria-label={t('Entity name')}
            disabled={!isEditingEntityName}
          />
          {!isEditingEntityName ? (
            <AppTooltip content={t('Edit name')} place="top">
              <button
                type="button"
                className="btn btn-outline-secondary"
                onClick={() => setIsEditingEntityName(true)}
              >
                <Pencil />
              </button>
            </AppTooltip>
          ) : (
            <AppTooltip content={t('Save changes')} place="top">
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
        isEnvironment ? (
          <div className="mb-2">
            <p className="prop-label">{t('Collision')}</p>
            <div className="d-flex align-items-center gap-2 mt-1">
              <input
                type="checkbox"
                id="environment-collision"
                className="form-check-input"
                checked={physicsEnabled}
                onChange={(e) => {
                  const next = e.target.checked
                  setEntityPhysics(selectedEntity.id, next, 'static')
                }}
              />
              <label htmlFor="environment-collision" className="form-check-label text-light small mb-0">
                {t('With collision')}
              </label>
            </div>
          </div>
        ) : isPlayer ? (
          <div className="mb-2">
            <p className="prop-label">{t('Physics')}</p>
            <div className="d-flex align-items-center gap-2 mt-1">
              <input
                type="checkbox"
                id="player-physics"
                className="form-check-input"
                checked
                disabled
                readOnly
              />
              <label htmlFor="player-physics" className="form-check-label text-light small mb-0">
                {t('Enable physics')}
              </label>
            </div>
            <select
              value="dynamic"
              className="form-select form-select-sm bg-dark text-light border-secondary mt-1"
              disabled
            >
              <option value="dynamic">{t('Dynamic (gravity)')}</option>
            </select>
            <p className="text-secondary small mb-0 mt-1">{t('Player physics managed by engine')}</p>
          </div>
        ) : (
          <div className="mb-2">
            <p className="prop-label">{t('Physics')}</p>
            <div className="d-flex align-items-center gap-2 mt-1">
              <input
                type="checkbox"
                id="physics-enabled"
                className="form-check-input"
                checked={physicsEnabled}
                onChange={(e) => {
                  const next = e.target.checked
                  const bodyType = next && isCharacter ? 'kinematic' : physicsType
                  setEntityPhysics(selectedEntity.id, next, bodyType)
                }}
              />
              <label htmlFor="physics-enabled" className="form-check-label text-light small mb-0">
                {t('Enable physics')}
              </label>
            </div>
            {physicsEnabled && (
              <select
                value={physicsType}
                className="form-select form-select-sm bg-dark text-light border-secondary mt-1"
                onChange={(e) => {
                  const next = e.target.value
                  setEntityPhysics(selectedEntity.id, true, next)
                }}
              >
                <option value="dynamic">{t('Dynamic (gravity)')}</option>
                <option value="static">{t('Static (does not move)')}</option>
                <option value="kinematic">{t('Kinematic (by code)')}</option>
              </select>
            )}
          </div>
        )
      )}

      {!isCollider && !isExecutionArea && (
        <Accordion className="prop-accordion">
          <Accordion.Item eventKey="transform">
            <Accordion.Header><ArrowsMove className="me-2" />{t('Transformations')}</Accordion.Header>
            <Accordion.Body className="py-2 px-2">
              <TransformPanel
                entity={selectedEntity}
                is2D={is2D}
                isPlayCharacter={isPlayer && is3D}
                isEditorCamera={isEditorCamera && is3D}
                onSend={handleSend}
              />
            </Accordion.Body>
          </Accordion.Item>

          {(is2D || hasEmbeddedModelClips) && <AnimationsPanel projectType={projectType} />}

          <ScriptingPanel />
        </Accordion>
      )}

      {isExecutionArea && (
        <Accordion className="prop-accordion">
          <ScriptingPanel />
        </Accordion>
      )}

      <div className="mt-3 pt-2 border-top border-secondary">
        {isFromBlueprint ? (
          <div className="d-flex align-items-center justify-content-center gap-2 mb-2 px-1 py-1 rounded small text-white">
            <Link45deg className="flex-shrink-0" />
            <span className="text-truncate">
              {linkedBlueprintName
                ? <>{t('Based on blueprint')}: <strong>{linkedBlueprintName}</strong></>
                : t('Based on blueprint')
              }
            </span>
          </div>
        ) : (
        <>
        {is3D && (
          <button
            className="btn btn-sm btn-outline-info w-100 mb-2"
            type="button"
            onClick={() => {
              openModal({
                title: t('Replace model'),
                body: (
                  <div>
                    <p className="text-secondary small">
                      {isPlayer ? t('Replace model player hint') : t('Replace model entity hint')}
                    </p>
                    <CreateEntityFromModelModalBody
                      models={models}
                      onSpawn={(path) => {
                        replaceEntityModel(selectedEntity.id, path);
                        closeModal();
                      }}
                    />
                  </div>
                ),
              });
            }}
          >
            <BoxSeam className="me-2" />
            {t('Replace model')}
          </button>
        )}
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
                rotation:         transform?.rotation ?? [0, 0, 0, 1],
                physics_enabled:  meta?.physicsEnabled,
                physics_type:     meta?.physicsType,
                animations:       meta?.animations,
                scripts:          meta?.scripts,
                control_bindings: meta?.controlBindings,
              };
              addBlueprint(entry);
              // Vincular la entidad actual a la blueprint recién creada
              if (entityMetaRef.current[selectedEntity.id]) {
                entityMetaRef.current[selectedEntity.id].blueprintId = entry.id;
              }
              closeModal();
            };

            openModal({
              title: t('Convert to Blueprint'),
              body: (
                <div className="text-center">
                  <CircleSquare size={40} className="text-primary mb-3" />
                  <p>{t('The entity will be converted to a Blueprint')} <strong>{selectedEntity.name}</strong>.</p>
                  <p className="text-secondary small">{t('The Blueprint will save the current entity configuration: transformations, physics, animations and scripts.')}</p>
                  <p className="text-secondary small">{t('The created Blueprint will be available in the Quick Build tool')}</p>
                  <div className="d-flex justify-content-center gap-2 mt-4">
                    <button className="btn btn-primary" onClick={handleConfirm}>
                      {t('Confirm')}
                    </button>
                    <button className="btn btn-secondary" onClick={closeModal}>
                      {t('Cancel')}
                    </button>
                  </div>
                </div>
              ),
            });
          }}
        >
          <CircleSquare className="me-2" />
          {t('Convert to Blueprint')}
        </button>
        </>
        )}
        {!isPlayer && (
        <button
          className="btn btn-sm btn-outline-danger w-100"
          onClick={handleRemove}
        >
          <Trash className="me-2" />
          {t('Delete')}
        </button>
        )}
      </div>
    </div>
  )
}

export default PropertiesAccordion;