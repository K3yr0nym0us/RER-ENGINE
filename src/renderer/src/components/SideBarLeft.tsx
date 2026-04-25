import { Accordion } from 'react-bootstrap';

import { PropertiesPanel } from './PropertiesPanel';
import { AssetGroupConfig, AssetGroupPanel, WorldPanel } from '../2D/components';

import { usePointDrawing } from '../hooks/usePointDrawing';

import { ProjectType } from '../../../shared-types/types';

const SCENARIO_CONFIG: AssetGroupConfig = {
  openDialog:  () => window.electronAPI.openScenarioDialog(),
  loadCmd:     'load_scenario',
  dupCmd:      'duplicate_scenario',
  addBtnLabel: '+ Agregar escenario (PNG)',
  emptyText:   'Sin escenarios cargados',
}

const CHARACTER_CONFIG: AssetGroupConfig = {
  openDialog:  () => window.electronAPI.openCharacterDialog(),
  loadCmd:     'load_character',
  dupCmd:      'duplicate_character',
  addBtnLabel: '+ Agregar personaje (PNG)',
  emptyText:   'Sin personajes cargados',
}

export function SideBarLeft({ projectType, engineReady, backgroundPath, worldConfig, toolProgress, colliderEntities, removeCollider, loadModel, setWorldSize, setGridVisible, setGridCellSize, scenarioEntities, removeScenario, duplicateScenario, characterEntities, removeCharacter, duplicateCharacter, hoveredEntityId, selectedEntity, send }: {
  projectType: ProjectType;
  engineReady: boolean;
  backgroundPath: string | null;
  worldConfig: any;
  setWorldSize: (width: number, height: number) => void;
  setGridVisible: (visible: boolean) => void;
  setGridCellSize: (cellSize: number) => void;
  scenarioEntities: any[];
  removeScenario: (id: number) => void;
  duplicateScenario: (id: number) => void;
  characterEntities: any[];
  removeCharacter: (id: number) => void;
  duplicateCharacter: (id: number) => void;
  hoveredEntityId: number | null;
  selectedEntity: any | null;
  send: (message: any) => void;
  toolProgress: number | null;
  loadModel: (path: string) => void;
  colliderEntities: any[];
  removeCollider: (id: number) => void;
}) {

  const colliderTool = usePointDrawing('draw_collider', 4, send, toolProgress)

  const loadBackground = () => {
    window.electronAPI.openBackgroundDialog().then((p: string | null) => {
      if (p) send({ cmd: 'load_background', path: p })
    })
  }

  return (
    <aside className="app-sidebar p-3 border-end border-secondary-subtle overflow-auto">
      <Accordion defaultActiveKey="mundo" className="sidebar-accordion">

        {projectType === '2D' && (
          <WorldPanel
            engineReady={engineReady}
            worldConfig={worldConfig}
            backgroundPath={backgroundPath ?? null}
            onWorldSize={setWorldSize}
            onGridVisible={setGridVisible}
            onCellSize={setGridCellSize}
            onLoadBackground={loadBackground}
          />
        )}

        <Accordion.Item eventKey="assets">
          <Accordion.Header>Assets</Accordion.Header>
          <Accordion.Body className="py-2 px-2">
            <Accordion>
              <Accordion.Item eventKey="escenarios">
                <Accordion.Header>Escenarios</Accordion.Header>
                <Accordion.Body className="py-2 px-2">
                  {projectType === '3D' && (
                    <button
                      className="btn btn-outline-light btn-sm w-100 fw-bold"
                      disabled={!engineReady}
                      onClick={() =>
                        window.electronAPI.openModelDialog().then((p: string | null) => { if (p) loadModel(p) })
                      }
                    >
                      Cargar modelo (.glb)
                    </button>
                  )}
                  {projectType === '2D' && (
                    <AssetGroupPanel
                      engineReady={engineReady}
                      send={send}
                      entries={scenarioEntities}
                      onRemove={removeScenario}
                      onDuplicate={duplicateScenario}
                      config={SCENARIO_CONFIG}
                      highlightId={hoveredEntityId ?? selectedEntity?.id ?? null}
                    />
                  )}
                </Accordion.Body>
              </Accordion.Item>
              <Accordion.Item eventKey="personajes">
                <Accordion.Header>Personajes</Accordion.Header>
                <Accordion.Body className="py-2 px-2">
                  {projectType === '2D' && (
                    <AssetGroupPanel
                      engineReady={engineReady}
                      send={send}
                      entries={characterEntities}
                      onRemove={removeCharacter}
                      onDuplicate={duplicateCharacter}
                      config={CHARACTER_CONFIG}
                      highlightId={hoveredEntityId ?? selectedEntity?.id ?? null}
                    />
                  )}
                </Accordion.Body>
              </Accordion.Item>
            </Accordion>
          </Accordion.Body>
        </Accordion.Item>

        <Accordion.Item eventKey="herramientas">
          <Accordion.Header>Herramientas</Accordion.Header>
          <Accordion.Body className="py-2 px-2">
            {!colliderTool.isActive ? (
              <button
                className="btn btn-sm btn-outline-info w-100 mb-2"
                onClick={colliderTool.start}
                disabled={!engineReady}
                title="Click 4 veces en el viewport para definir un cuadro de colisiones"
              >
                ⬡ Cuadro de colisiones
              </button>
            ) : (
              <div className="mb-2">
                <div className="text-info small mb-1">
                  Cuadro de colisiones — {colliderTool.progress}/{colliderTool.totalPoints} puntos colocados
                </div>
                <div className="d-flex gap-1 mb-2">
                  {[...Array(colliderTool.totalPoints)].map((_, i) => (
                    <div
                      key={i}
                      style={{
                        width:           12,
                        height:          12,
                        borderRadius:    '50%',
                        border:          '1px solid var(--bs-info)',
                        backgroundColor: i < colliderTool.progress ? 'var(--bs-info)' : 'transparent',
                      }}
                    />
                  ))}
                </div>
                <button
                  className="btn btn-sm btn-outline-danger w-100"
                  onClick={colliderTool.cancel}
                >
                  Cancelar
                </button>
              </div>
            )}
            {colliderEntities.length > 0 && (
              <div className="mt-1">
                <div className="text-muted small mb-1">Colisionadores ({colliderEntities.length})</div>
                {colliderEntities.map((c) => (
                  <div key={c.id} className="d-flex align-items-center justify-content-between mb-1">
                    <span className="small text-truncate me-1">#{c.id}</span>
                    <button
                      className="btn btn-sm btn-outline-danger py-0 px-1"
                      style={{ fontSize: '0.7rem' }}
                      onClick={() => removeCollider(c.id)}
                      title="Eliminar colisionador"
                    >
                      ✕
                    </button>
                  </div>
                ))}
              </div>
            )}
          </Accordion.Body>
        </Accordion.Item>
      </Accordion>

      {selectedEntity && !colliderEntities.some((c) => c.id === selectedEntity.id) && (
        <div className="pt-4">
          <b className="ms-2">Elemento seleccionado:</b>
          <Accordion defaultActiveKey="propiedades" className="sidebar-accordion mt-1">
            <Accordion.Item eventKey="propiedades">
              <Accordion.Header>Propiedades</Accordion.Header>
              <Accordion.Body className="py-2 px-1">
                <PropertiesPanel
                  entity={selectedEntity ?? null}
                  onSend={send}
                  projectType={projectType}
                  isScenario={scenarioEntities.some((s) => s.id === selectedEntity?.id)}
                />
              </Accordion.Body>
            </Accordion.Item>
          </Accordion>
        </div>
      )}
    </aside>
  )
}

export default SideBarLeft;
