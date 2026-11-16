import type { ModalElectronOpenRequest } from '@shared-types'
import type { EngineContextValue } from '@engine'
import {
	resolveSceneEntitiesForVisualScript,
	sanitizeSceneEntitiesForModal,
} from '../visualScripting/resolveSceneEntities'
import { serializeModalProps } from './modalElectronSerialize'
import { buildEntityPropertiesState } from './entityPropertiesModalSessions'

export function buildEngineSnapshot(
	componentKey: string,
	engine: EngineContextValue,
): Partial<ModalElectronOpenRequest> {
	switch (componentKey) {
		case 'ModalSelectFont':
			return {
				fonts: serializeModalProps({ fonts: engine.fonts }).fonts as ModalElectronOpenRequest['fonts'],
			}
		case 'ModalSelectHudImage':
			return {
				hudImages: serializeModalProps({ hudImages: engine.hudImages })
					.hudImages as ModalElectronOpenRequest['hudImages'],
			}
		case 'CreateEntityFromSpriteModalBody':
		case 'CreateProjectileFromSpriteModalBody':
			return {
				sprites: serializeModalProps({ sprites: engine.sprites })
					.sprites as ModalElectronOpenRequest['sprites'],
			}
		case 'CreateEntityFromModelModalBody':
			return {
				models: serializeModalProps({ models: engine.models })
					.models as ModalElectronOpenRequest['models'],
			}
		case 'BluePrintModalBody':
			return {
				blueprints: JSON.parse(JSON.stringify(engine.blueprints)) as typeof engine.blueprints,
				linkedEntityCounts: buildLinkedEntityCounts(engine),
			}
		case 'VisualScriptingModalBody':
			return {
				blueprints: JSON.parse(JSON.stringify(engine.blueprints)) as typeof engine.blueprints,
				sceneEntities: sanitizeSceneEntitiesForModal(
					resolveSceneEntitiesForVisualScript({
						entityMeta: engine.entityMetaRef.current,
						entityTransforms: engine.entityTransformsRef.current,
					}),
				),
			}
		case 'EntityPropertiesModalBody':
			return {
				entityPropertiesState: JSON.parse(
					JSON.stringify(buildEntityPropertiesState(engine)),
				),
				sprites: serializeModalProps({ sprites: engine.sprites })
					.sprites as ModalElectronOpenRequest['sprites'],
				models: serializeModalProps({ models: engine.models })
					.models as ModalElectronOpenRequest['models'],
				blueprints: JSON.parse(JSON.stringify(engine.blueprints)) as typeof engine.blueprints,
			}
		default:
			return {}
	}
}

function buildLinkedEntityCounts(engine: EngineContextValue): Record<string, number> {
	const counts: Record<string, number> = {}
	for (const bp of engine.blueprints) {
		counts[bp.id] = Object.entries(engine.entityMetaRef.current)
			.filter(([, meta]) => meta.blueprintId === bp.id)
			.length
	}
	return counts
}
