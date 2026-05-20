import type { Dispatch } from 'react';
import type { Locale } from '../../LanguageContext';
import type {
	AnimationLogicalResolved,
	AnimationFinished,
	Camera2dUpdated,
	CharacterLoaded,
	DebugMetrics,
	EntitySelected,
	PhysicsChanged,
	PivotSelected,
	ScenarioLoaded,
	SpriteRemoved,
	SpritesList,
} from '@shared-types';
import type { GameStyle, SavedScene } from '@shared-types';
import {
	DEFAULT_LIGHT_AMBIENT,
	DEFAULT_LIGHT_INTENSITY,
	DEFAULT_SHADOW_DARKNESS,
	FIRST_PERSON_PLAYER_BODY_SCALE,
	isEditorBoxPath,
	isPlayerEntity,
	isPlayerPath,
	isGroundPath,
	isSunPath,
	type EntityCategory,
} from '@shared-types';
import { applyPlayCharacterControlDefaultsIfEmpty } from '../../../defaults/applyPlayCharacterControlDefaults';
import {
	applyPlayCharacterViewFromEngine,
	applySavedPlayCharacterView,
	ensurePlayCharacterOnLoad,
	type PlayCharacterViewChangedEvent,
} from '../../../defaults/playCharacterSceneRestore';
import { setSceneCommandForSavedProject } from '../../../defaults/projectSceneLoad';
import { buildImportSceneCommand, resolveEntityTransform, syncEditorStateFromSavedScene } from './buildImportSceneCommand';
import { applyPendingRestoreMeta, buildPlayAnimationFrameCmd, sendApplyEntityRestore } from './applyPendingRestoreToEngine';
import {
	beginSceneBurstLoad,
	beginSceneImportLoading,
	endSceneBurstLoad,
	endSceneImportLoading,
	needsSceneBurstLoad,
	trackSceneBurstCollider,
	tryEndSceneBurstLoad,
} from './sceneImportOverlay';
import type { EngineAction, EngineInternalRefs, PendingRestore, Transform } from '../types';

type RuntimeEngineEvent = {
	event: string
	[key: string]: unknown
};

const SILENT_ENGINE_EVENTS = new Set<string>([
	'debug_metrics',
	'animation_finished',
	'ready',
	'player_ready',
	'character_loaded',
	'sprite_loaded',
	'background_loaded',
	'sound_loaded',
	'background_asset_loaded',
	'scenario_loaded',
	'scene_imported',
	'collider_created',
	'execution_area_created',
	'entity_deselected',
	'entity_hovered',
	'entity_unhovered',
	'entity_selected',
	'physics_changed',
	'quick_build_move',
	'camera_2d_updated',
	'animation_logical_resolved',
	'multi_selection_transformed',
	'autosave_tick',
	'atlas_exhausted',
	'preview_playing_changed',
	'play_character_view_changed', 'first_person_view_changed',
	'save_snapshot_ready',
]);

interface CreateEngineEventHandlerParams {
	dispatch: Dispatch<EngineAction>
	refs: EngineInternalRefs
	addLog: (text: string, isError?: boolean) => void
	projectType?: string
	gameStyle?: GameStyle
	applyInitialAnimationFrame: (entityId: number, animations?: any[]) => void
	setLocale?: (locale: Locale) => void
	reportBounds: () => void
}

function applyPlayCharacterDefaultsForPlayer(
	characterId: number,
	gameStyle: GameStyle | undefined,
	refs: EngineInternalRefs,
) {
	if (gameStyle !== 'first-person') return;
	// Proyecto abierto desde .save: bindings y entidades vienen del archivo, no de la plantilla.
	if (refs.initialSaveRef.current) return;

	applyPlayCharacterControlDefaultsIfEmpty(characterId, refs.entityMetaRef, (cmd) => {
		window.engine.send(cmd as never);
	});
}

export function createEngineEventHandler({
	dispatch,
	refs,
	addLog,
	projectType,
	gameStyle,
	applyInitialAnimationFrame,
	setLocale,
	reportBounds,
}: CreateEngineEventHandlerParams) {
	const buildTransformFromPoints = (
		points?: [[number, number], [number, number], [number, number], [number, number]],
	): Transform | null => {
		if (!points || points.length !== 4) return null;
		const xs = points.map(([x]) => x);
		const ys = points.map(([, y]) => y);
		const minX = Math.min(...xs);
		const maxX = Math.max(...xs);
		const minY = Math.min(...ys);
		const maxY = Math.max(...ys);
		const bw = Math.max(0.01, maxX - minX);
		const bh = Math.max(0.01, maxY - minY);
		const cx = (minX + maxX) * 0.5;
		const cy = (minY + maxY) * 0.5;
		return {
			position: [cx, cy, -0.5],
			rotation: [0, 0, 0, 1],
			scale: [bw, bh, 1],
		};
	};

	return (event: RuntimeEngineEvent) => {
		// Eventos de alta frecuencia: se procesan normalmente, pero no se
		// imprimen en la consola del panel para evitar spam visual.
		if (!SILENT_ENGINE_EVENTS.has(event.event)) {
			addLog(JSON.stringify(event), event.event === 'error');
		}

		const pendingEvent = refs.pendingEventsRef.current.get(event.event);
		if (pendingEvent) {
			pendingEvent.resolve(event);
			refs.pendingEventsRef.current.delete(event.event);
		}

		if (event.event === 'ready') {
			dispatch({ type: 'SET_READY' });
			dispatch({ type: 'SET_PREVIEW_PLAYING', payload: false });
			if (refs.readyTimer.current) clearTimeout(refs.readyTimer.current);
			const sendEngine = window.engine.send;
			const baseSave = refs.initialSaveRef.current;
			if (projectType) {
				if (baseSave) {
					window.engine.send({
						cmd: 'set_scene',
						scene: setSceneCommandForSavedProject(projectType),
					} as never);
				} else if (gameStyle === 'first-person' && projectType === '3D') {
					window.engine.send({ cmd: 'set_scene', scene: 'first-person' } as never);
				} else {
					window.engine.send({ cmd: 'set_scene', scene: projectType } as never);
				}
			}
			window.engine.send({ cmd: 'set_preview_playing', playing: false } as never);
			refs.mainPlayerHandled.current = false;
			refs.playerRemoved.current = false;
			refs.pendingPlayerDups.current = [];
			refs.pendingDupQ.current = [];
			let savedGravity: number | undefined;
			if (baseSave) {
				const scenes = baseSave.scenes ?? [];
				const activeScene = scenes.length > 0
					? (scenes.find((scene) => scene.id === baseSave.activeSceneId) ?? scenes[0])
					: null;

				const save = activeScene
					? {
						...baseSave,
						world: activeScene.world,
						backgroundPath: activeScene.backgroundPath,
						entities: activeScene.entities,
						playerTransform: activeScene.playerTransform,
						camera2d: activeScene.camera2d,
						sprites: activeScene.sprites,
						models: activeScene.models,
					}
					: baseSave;

				refs.initialSaveRef.current = save;
				if (gameStyle === 'first-person' && projectType === '3D' && save.playerTransform) {
					refs.pendingPlayCharacterViewRef.current = save.playerTransform;
					refs.playCharacterViewRef.current = save.playerTransform;
				}

				const importScene2d = projectType === '2D' && (save.entities?.length ?? 0) > 0;

				if (save.world) {
					savedGravity = save.world.gravity;
					const worldPayload = {
						...save.world,
						lightAmbient: save.world.lightAmbient ?? DEFAULT_LIGHT_AMBIENT,
						lightIntensity: save.world.lightIntensity ?? DEFAULT_LIGHT_INTENSITY,
						shadowDarkness: save.world.shadowDarkness ?? DEFAULT_SHADOW_DARKNESS,
					};
					dispatch({ type: 'SET_WORLD_CONFIG', payload: worldPayload });
					if (!importScene2d) {
						sendEngine({
							cmd: 'set_world_size',
							width: save.world.worldWidth,
							height: save.world.worldHeight,
							depth: save.world.worldDepth,
						} as never);
						sendEngine({ cmd: 'set_grid_visible', visible: save.world.gridVisible } as never);
						sendEngine({ cmd: 'set_grid_cell_size', size: save.world.gridCellSize } as never);
						sendEngine({ cmd: 'set_target_fps', fps: save.world.targetFps } as never);
						if (save.world.gravity != null) {
							sendEngine({ cmd: 'set_gravity', gravity: save.world.gravity } as never);
						}
						if (projectType === '3D') {
							sendEngine({
								cmd: 'set_directional_light',
								ambient: worldPayload.lightAmbient,
								intensity: worldPayload.lightIntensity,
								shadow_darkness: worldPayload.shadowDarkness,
							} as never);
						}
					}
				}
			if (save.language && (save.language === 'en' || save.language === 'es')) {
				const validLocale = save.language as Locale;
				setLocale?.(validLocale);
			}
			if (save.camera2d) {
				refs.camera2dRef.current = save.camera2d;
				if (!importScene2d) {
					sendEngine({ cmd: 'set_camera2d', x: save.camera2d.x, y: save.camera2d.y, half_h: save.camera2d.halfH } as never);
				}
			}
				if (!importScene2d && save.sprites && save.sprites.length > 0) {
					for (const sprite of save.sprites) {
						sendEngine({ cmd: 'load_sprite', path: sprite.path, name: sprite.name } as never);
						dispatch({ type: 'ADD_SPRITE_INFO', payload: { path: sprite.path, name: sprite.name } });
					}
				}
				if (save.models && save.models.length > 0) {
					for (const model of save.models) {
						sendEngine({ cmd: 'load_model_asset', path: model.path, name: model.name } as never);
						dispatch({ type: 'ADD_MODEL_INFO', payload: { path: model.path, name: model.name } });
					}
				}
				if (save.sounds && save.sounds.length > 0) {
					for (const sound of save.sounds) {
						sendEngine({ cmd: 'load_sound', path: sound.path, name: sound.name } as never);
					}
					dispatch({ type: 'SET_SOUNDS', payload: save.sounds });
				}
				if (save.backgrounds && save.backgrounds.length > 0) {
					for (const bg of save.backgrounds) {
						sendEngine({ cmd: 'load_background_asset', path: bg.path, name: bg.name } as never);
					}
					dispatch({ type: 'SET_BACKGROUNDS', payload: save.backgrounds });
				}
				if (save.backgroundPath && !importScene2d) {
					sendEngine({ cmd: 'load_background', path: save.backgroundPath } as never);
				}
				let burstLoad = false;
				if (importScene2d) {
					const sceneForImport: SavedScene = {
						id: activeScene?.id ?? 1,
						name: activeScene?.name ?? 'Escena',
						world: save.world!,
						backgroundPath: save.backgroundPath,
						entities: save.entities,
						playerTransform: save.playerTransform,
						camera2d: save.camera2d,
						sprites: save.sprites ?? [],
					};
					refs.pendingImportSceneRef.current = sceneForImport;
					refs.pendingRestoresRef.current.clear();
					beginSceneImportLoading(dispatch, refs.sceneImportInProgressRef);
					sendEngine(
						buildImportSceneCommand(
							sceneForImport,
							save.blueprints ?? refs.blueprintsRef.current,
						) as never,
					);
				} else {
				burstLoad = needsSceneBurstLoad(projectType, gameStyle, save);
				if (burstLoad) {
					refs.sceneBurstAwaitingPlayerViewRef.current = false;
					refs.sceneBurstPendingColliderCountRef.current = 0;
					beginSceneBurstLoad(dispatch, refs.sceneBurstLoadInProgressRef);
				}
				const loadBlueprints = save.blueprints ?? refs.blueprintsRef.current;
				for (const entity of save.entities) {
					const transform = resolveEntityTransform(entity, loadBlueprints);
					if (entity.kind === 'collider' && entity.points) {
						if (burstLoad) trackSceneBurstCollider(refs);
						sendEngine({ cmd: 'create_collider_from_points', points: entity.points, track_undo: false } as never);
					} else if (entity.kind === 'execution_area' && entity.points) {
						const pendingRestore: PendingRestore = {
							transform,
							name: entity.name,
							physicsEnabled: entity.physics_enabled ?? false,
							physicsType: entity.physics_type ?? 'static',
							scripts: entity.scripts,
						};
						const queue = refs.pendingRestoresRef.current.get('[ExecutionArea]') ?? [];
						queue.push(pendingRestore);
						refs.pendingRestoresRef.current.set('[ExecutionArea]', queue);
						sendEngine({ cmd: 'create_execution_area_from_points', points: entity.points, track_undo: false } as never);
					} else if (entity.kind === 'character' && isPlayerPath(entity.path)) {
						const savedPlayer = save.playerTransform;
						if (savedPlayer) {
							refs.pendingPlayCharacterViewRef.current = savedPlayer;
						}
						const pendingRestore: PendingRestore = {
							transform,
							name: entity.name,
							physicsEnabled: true,
							physicsType: 'dynamic',
							scripts: entity.scripts,
							controlBindings: savedPlayer?.control_bindings ?? entity.control_bindings,
							visualModelPath: savedPlayer?.visual_model_path ?? entity.visual_model_path,
						};
						const queue = refs.pendingRestoresRef.current.get('[Player]') ?? [];
						queue.push(pendingRestore);
						refs.pendingRestoresRef.current.set('[Player]', queue);
						sendEngine({ cmd: 'load_character', path: entity.path } as never);
					} else if (entity.kind === 'directional_light' || isSunPath(entity.path)) {
						const pendingRestore: PendingRestore = {
							transform,
							name: entity.name,
							physicsEnabled: false,
							physicsType: 'static',
							scripts: entity.scripts,
							controlBindings: entity.control_bindings,
							blueprintId: entity.blueprint_id,
						};
						const queue = refs.pendingRestoresRef.current.get('[Sun]') ?? [];
						queue.push(pendingRestore);
						refs.pendingRestoresRef.current.set('[Sun]', queue);
						sendEngine({
							cmd: 'spawn_sun',
							name: entity.name ?? 'Sol',
							position: entity.position,
							scale: entity.scale,
						} as never);
					} else if (entity.kind === 'model' && isGroundPath(entity.path)) {
						const pendingRestore: PendingRestore = {
							transform,
							name: entity.name,
							physicsEnabled: false,
							physicsType: 'static',
							scripts: entity.scripts,
							controlBindings: entity.control_bindings,
						};
						const queue = refs.pendingRestoresRef.current.get('[Ground]') ?? [];
						queue.push(pendingRestore);
						refs.pendingRestoresRef.current.set('[Ground]', queue);
						sendEngine({
							cmd: 'spawn_ground',
							position: entity.position,
							scale: entity.scale,
						} as never);
					} else if (entity.kind === 'model' && isEditorBoxPath(entity.path)) {
						const pendingRestore: PendingRestore = {
							transform,
							name: entity.name,
							physicsEnabled: entity.physics_enabled ?? false,
							physicsType: entity.physics_type ?? 'static',
							scripts: entity.scripts,
							controlBindings: entity.control_bindings,
							blueprintId: entity.blueprint_id,
						};
						const queue = refs.pendingRestoresRef.current.get('[EditorBox]') ?? [];
						queue.push(pendingRestore);
						refs.pendingRestoresRef.current.set('[EditorBox]', queue);
						sendEngine({
							cmd: 'spawn_editor_box',
							name: entity.name ?? 'Box',
							position: entity.position,
							scale: entity.scale,
						} as never);
					} else {
						// Si la entidad es una instancia de blueprint, heredar las propiedades
						// del blueprint original en lugar de las guardadas por entidad.
						const bp = entity.blueprint_id
							? (save.blueprints ?? []).find((b) => b.id === entity.blueprint_id) ?? null
							: null;
						const pendingRestore: PendingRestore = {
							transform,
							name: entity.name,
							physicsEnabled: bp?.physics_enabled ?? entity.physics_enabled ?? false,
							physicsType: bp?.physics_type ?? entity.physics_type ?? 'static',
							animations: bp?.animations ?? entity.animations,
							scripts: bp?.scripts ?? entity.scripts,
							controlBindings: bp?.control_bindings ?? entity.control_bindings,
							blueprintId: entity.blueprint_id,
							entityCategory: entity.entity_category,
							visualModelPath: entity.visual_model_path,
						};
						const queue = refs.pendingRestoresRef.current.get(entity.path) ?? [];
						queue.push(pendingRestore);
						refs.pendingRestoresRef.current.set(entity.path, queue);
						if (entity.kind === 'scenario') sendEngine({ cmd: 'load_scenario', path: entity.path } as never);
						if (entity.kind === 'character') sendEngine({ cmd: 'load_character', path: entity.path } as never);
						if (entity.kind === 'model' && entity.path && !isEditorBoxPath(entity.path)) {
							refs.pendingModelLoadQueueRef.current.push({
								modelPath: entity.path,
								pending: pendingRestore,
							});
							sendEngine({ cmd: 'load_model', path: entity.path } as never);
						}
					}
				}
				}
				if (gameStyle === 'first-person' && projectType === '3D') {
					ensurePlayCharacterOnLoad(save, refs.pendingRestoresRef, (cmd) => sendEngine(cmd as never));
				}
				if (burstLoad) {
					setTimeout(
						() => tryEndSceneBurstLoad(
							dispatch,
							refs.sceneBurstLoadInProgressRef,
							refs,
							reportBounds,
						),
						0,
					);
				}
			}
			const motorGravity = typeof event.gravity === 'number' ? event.gravity : undefined;
			if (motorGravity != null && savedGravity == null) {
				dispatch({ type: 'SET_WORLD_CONFIG', payload: { gravity: motorGravity } });
			}
		}

		if (event.event === 'model_loaded') {
			const loaded = event as {
				id?: number
				name?: string
				position?: [number, number, number]
				scale?: [number, number, number]
			};
			const id = loaded.id ?? -1;
			dispatch({ type: 'ADD_ENTITY', payload: id });
			if (loaded.position && loaded.scale) {
				refs.entityTransformsRef.current[id] = {
					position: loaded.position,
					rotation: [0, 0, 0, 1],
					scale: loaded.scale,
				};
			}
			const sunQueue = refs.pendingRestoresRef.current.get('[Sun]');
			if (sunQueue && sunQueue.length > 0) {
				const pending = sunQueue.shift()!;
				refs.entityMetaRef.current[id] = {
					kind: 'directional_light',
					path: '[Sun]',
					name: pending.name ?? loaded.name ?? `Entity ${id}`,
					physicsEnabled: false,
					physicsType: 'static',
				};
				sendApplyEntityRestore(id, pending, {
					skipTransform: true,
					applyInitialAnimationFrame: false,
				});
				applyPendingRestoreMeta(refs, id, pending);
				if (sunQueue.length === 0) refs.pendingRestoresRef.current.delete('[Sun]');
			} else {
			const groundQueue = refs.pendingRestoresRef.current.get('[Ground]');
			if (groundQueue && groundQueue.length > 0) {
				const pending = groundQueue.shift()!;
				refs.entityMetaRef.current[id] = {
					kind: 'model',
					path: '[Ground]',
					name: pending.name ?? loaded.name ?? 'Ground',
					physicsEnabled: false,
					physicsType: 'static',
				};
				sendApplyEntityRestore(id, pending, {
					skipTransform: true,
					applyInitialAnimationFrame: false,
				});
				applyPendingRestoreMeta(refs, id, pending);
				if (groundQueue.length === 0) refs.pendingRestoresRef.current.delete('[Ground]');
			} else if (
				loaded.name?.toLowerCase() === 'ground'
				&& !refs.entityMetaRef.current[id]
			) {
				refs.entityMetaRef.current[id] = {
					kind: 'model',
					path: '[Ground]',
					name: 'Ground',
					physicsEnabled: false,
					physicsType: 'static',
				};
			} else {
			const editorBoxQueue = refs.pendingRestoresRef.current.get('[EditorBox]');
			if (editorBoxQueue && editorBoxQueue.length > 0) {
				const pending = editorBoxQueue.shift()!;
				refs.entityMetaRef.current[id] = {
					kind: 'model',
					path: '[EditorBox]',
					name: pending.name ?? loaded.name ?? `Entity ${id}`,
					physicsEnabled: pending.physicsEnabled,
					physicsType: pending.physicsType ?? 'static',
				};
				sendApplyEntityRestore(id, pending, {
					skipTransform: true,
					applyInitialAnimationFrame: false,
				});
				applyPendingRestoreMeta(refs, id, pending);
				if (editorBoxQueue.length === 0) refs.pendingRestoresRef.current.delete('[EditorBox]');
			} else {
				const spawnModelPath = refs.pendingModelPathRef.current;
				const spawnKind = refs.pendingSpawnKindRef.current ?? 'model';
				const spawnCategory = refs.pendingSpawnCategoryRef.current;
				const loadItem = !spawnModelPath
					? refs.pendingModelLoadQueueRef.current.shift()
					: null;
				const modelPath = spawnModelPath ?? loadItem?.modelPath ?? null;
				const restorePending = loadItem?.pending;

				if (modelPath) {
					const isEnvironment = spawnCategory === 'environment'
						|| restorePending?.entityCategory === 'environment';
					refs.entityMetaRef.current[id] = {
						kind: spawnKind,
						path: modelPath,
						name: restorePending?.name ?? loaded.name ?? `Entity ${id}`,
						physicsEnabled: isEnvironment
							? true
							: (restorePending?.physicsEnabled ?? true),
						physicsType: isEnvironment
							? 'static'
							: (restorePending?.physicsType ?? 'static'),
						...(isEnvironment ? { entityCategory: 'environment' as EntityCategory } : {}),
						...(restorePending?.entityCategory ? { entityCategory: restorePending.entityCategory } : {}),
						scripts: restorePending?.scripts,
						controlBindings: restorePending?.controlBindings,
						blueprintId: restorePending?.blueprintId,
						visualModelPath: restorePending?.visualModelPath,
					};
					if (restorePending?.name && restorePending.name.trim().length > 0) {
						window.engine.send({
							cmd: 'set_entity_name',
							id,
							name: restorePending.name,
							force: true,
						} as never);
					}
					if (restorePending?.transform) {
						window.engine.send({
							cmd: 'set_transform',
							id,
							position: restorePending.transform.position,
							rotation: restorePending.transform.rotation,
							scale: restorePending.transform.scale,
							track_undo: false,
						} as never);
						refs.entityTransformsRef.current[id] = restorePending.transform;
					}
					if (spawnKind === 'character') {
						dispatch({
							type: 'ADD_CHARACTER',
							payload: { id, path: modelPath },
						});
					}
					if (isEnvironment && projectType === '3D') {
						window.engine.send({
							cmd: 'set_physics',
							id,
							enabled: true,
							body_type: 'static',
						} as never);
					} else if (restorePending?.physicsEnabled) {
						window.engine.send({
							cmd: 'set_physics',
							id,
							enabled: true,
							body_type: restorePending.physicsType,
						} as never);
					}
					const visualPath = restorePending?.visualModelPath;
					if (visualPath && visualPath !== modelPath) {
						window.engine.send({
							cmd: 'replace_entity_model',
							id,
							path: visualPath,
						} as never);
					}
					refs.pendingModelPathRef.current = null;
					refs.pendingSpawnKindRef.current = null;
					refs.pendingSpawnCategoryRef.current = null;
				} else if (!refs.entityMetaRef.current[id]) {
					refs.entityMetaRef.current[id] = {
						kind: 'model',
						path: '[EditorBox]',
						name: loaded.name ?? `Entity ${id}`,
						physicsEnabled: true,
						physicsType: 'static',
					};
				} else if (loaded.name) {
					refs.entityMetaRef.current[id].name = loaded.name;
					if (!refs.entityMetaRef.current[id].path || refs.entityMetaRef.current[id].path === '') {
						refs.entityMetaRef.current[id].path = '[EditorBox]';
					}
				}
			}
			}
			}
			tryEndSceneBurstLoad(dispatch, refs.sceneBurstLoadInProgressRef, refs, reportBounds);
		}

		if (event.event === 'entity_model_replaced') {
			const replaced = event as {
				id?: number
				path?: string
				position?: [number, number, number]
				rotation?: [number, number, number, number]
				scale?: [number, number, number]
			};
			const id = replaced.id ?? -1;
			if (replaced.path && refs.entityMetaRef.current[id]) {
				refs.entityMetaRef.current[id].visualModelPath = replaced.path;
			}
			if (replaced.position && replaced.scale) {
				refs.entityTransformsRef.current[id] = {
					position: replaced.position,
					rotation: replaced.rotation ?? refs.entityTransformsRef.current[id]?.rotation ?? [0, 0, 0, 1],
					scale: replaced.scale,
				};
			}
			if (refs.playerEntityIdRef.current === id) {
				const meta = refs.entityMetaRef.current[id];
				if (meta) {
					meta.physicsEnabled = true;
					meta.physicsType = 'dynamic';
					if (meta.controlBindings) {
						window.engine.send({
							cmd: 'set_control_bindings',
							id,
							bindings: meta.controlBindings,
						} as never);
					}
				}
			}
			if (
				refs.sceneBurstLoadInProgressRef.current
				&& !(gameStyle === 'first-person' && projectType === '3D' && refs.playerEntityIdRef.current === id)
			) {
				tryEndSceneBurstLoad(dispatch, refs.sceneBurstLoadInProgressRef, refs, reportBounds);
			}
		}

		if (event.event === 'entity_selected') {
			const selected = event as unknown as EntitySelected;
			const meta = refs.entityMetaRef.current[selected.id];
			const bpId = meta?.blueprintId;
			if (bpId) {
				const bp = refs.blueprintsRef.current.find((b) => b.id === bpId);
				if (bp) {
					const bpRot = bp.rotation ?? [0, 0, 0, 1];
					const scaleChanged =
						Math.abs(selected.scale[0] - bp.scale[0]) > 1e-4
						|| Math.abs(selected.scale[1] - bp.scale[1]) > 1e-4
						|| Math.abs(selected.scale[2] - bp.scale[2]) > 1e-4;
					const rotChanged =
						Math.abs(selected.rotation[0] - bpRot[0]) > 1e-4
						|| Math.abs(selected.rotation[1] - bpRot[1]) > 1e-4
						|| Math.abs(selected.rotation[2] - bpRot[2]) > 1e-4
						|| Math.abs(selected.rotation[3] - bpRot[3]) > 1e-4;
					if (scaleChanged || rotChanged) {
						refs.updateEntityTransformRef.current(selected.id, {
							position: selected.position,
							...(scaleChanged ? { scale: selected.scale } : {}),
							...(rotChanged ? { rotation: selected.rotation } : {}),
						});
					} else {
						refs.entityTransformsRef.current[selected.id] = {
							position: selected.position,
							rotation: selected.rotation,
							scale: selected.scale,
						};
					}
				} else {
					refs.entityTransformsRef.current[selected.id] = {
						position: selected.position,
						rotation: selected.rotation,
						scale: selected.scale,
					};
				}
			} else {
				refs.entityTransformsRef.current[selected.id] = {
					position: selected.position,
					rotation: selected.rotation,
					scale: selected.scale,
				};
			}
			const isPlayer = isPlayerEntity(selected.id, meta, refs.playerEntityIdRef.current);
			const physicsEnabled = isPlayer
				? true
				: (selected.physics_enabled ?? false);
			const physicsType = isPlayer
				? 'dynamic'
				: (selected.physics_type ?? '');
			if (meta) {
				meta.name = selected.name;
				meta.physicsEnabled = physicsEnabled;
				meta.physicsType = physicsType;
			}
			dispatch({
				type: 'SELECT_ENTITY',
				payload: {
					id: selected.id,
					name: selected.name,
					position: selected.position,
					rotation: selected.rotation,
					scale: selected.scale,
					physicsEnabled,
					physicsType,
					path: meta?.path,
					animations: meta?.animations,
					scripts: meta?.scripts,
				},
			});
		}

		if (event.event === 'entity_deselected') {
			dispatch({ type: 'DESELECT_ENTITY' });
		}

		if (event.event === 'entity_hovered') {
			dispatch({ type: 'SET_HOVER', payload: (event as { id?: number }).id ?? null });
		}

		if (event.event === 'entity_unhovered') {
			dispatch({ type: 'SET_HOVER', payload: null });
		}


		if (event.event === 'camera_2d_updated') {
			const cameraUpdated = event as unknown as Camera2dUpdated;
			refs.camera2dRef.current = { x: cameraUpdated.x, y: cameraUpdated.y, halfH: cameraUpdated.half_h };
		}

		if (event.event === 'background_loaded') {
			dispatch({ type: 'SET_BACKGROUND', payload: (event as { path?: string }).path ?? null });
		}

		if (event.event === 'scene_imported') {
			const scene = refs.pendingImportSceneRef.current;
			if (scene) {
				syncEditorStateFromSavedScene(
					scene,
					refs,
					dispatch,
					refs.blueprintsRef.current,
				);
				if (scene.sprites?.length) {
					dispatch({ type: 'SET_LOADED_SPRITES_INFO', payload: scene.sprites });
				}
				window.engine.send({ cmd: 'get_sprites_list' } as never);
				dispatch({ type: 'SYNC_PLAY_CHARACTER_VIEW' });
			}
			endSceneImportLoading(
				dispatch,
				refs.sceneImportInProgressRef,
				refs.pendingImportSceneRef,
				reportBounds,
			);
		}

		if (event.event === 'scenario_loaded') {
			if (refs.sceneImportInProgressRef.current) return;
			const scenario = event as unknown as ScenarioLoaded;
			dispatch({ type: 'ADD_SCENARIO', payload: { id: scenario.id, path: scenario.path } });
			refs.entityMetaRef.current[scenario.id] = { kind: 'scenario', path: scenario.path, physicsEnabled: false, physicsType: '' };
			const queue = refs.pendingRestoresRef.current.get(scenario.path);
			if (queue && queue.length > 0) {
				const pending = queue.shift()!;
				sendApplyEntityRestore(scenario.id, pending);
				applyPendingRestoreMeta(refs, scenario.id, pending);
				if (queue.length === 0) refs.pendingRestoresRef.current.delete(scenario.path);
			}
			tryEndSceneBurstLoad(dispatch, refs.sceneBurstLoadInProgressRef, refs, reportBounds);
		}

		if (event.event === 'character_loaded') {
			if (refs.sceneImportInProgressRef.current) return;
			const character = event as unknown as CharacterLoaded;
			const applyPendingRestore = (
				id: number,
				path: string,
				options?: { skipTransform?: boolean },
			) => {
				const queue = refs.pendingRestoresRef.current.get(path);
				if (!queue || queue.length === 0) return;

				const pending = queue.shift()!;
				const isPlayer = isPlayerPath(path);
				sendApplyEntityRestore(id, pending, {
					omitScale: isPlayer,
					skipTransform: options?.skipTransform,
				});
				applyPendingRestoreMeta(refs, id, pending);

				if (pending.visualModelPath) {
					window.engine.send({
						cmd: 'replace_entity_model',
						id,
						path: pending.visualModelPath,
					} as never);
					if (refs.entityMetaRef.current[id]) {
						refs.entityMetaRef.current[id].visualModelPath = pending.visualModelPath;
					}
					if (
						refs.sceneBurstLoadInProgressRef.current
						&& isPlayer
						&& gameStyle === 'first-person'
						&& projectType === '3D'
					) {
						refs.sceneBurstAwaitingPlayerViewRef.current = true;
					}
				}

				if (queue.length === 0) refs.pendingRestoresRef.current.delete(path);
			};

			if (isPlayerPath(character.path)) {
				if (!refs.mainPlayerHandled.current) {
					refs.mainPlayerHandled.current = true;
					if (!refs.playerRemoved.current) {
						dispatch({ type: 'ADD_CHARACTER', payload: { id: character.id, path: character.path } });
					}
					refs.playerRemoved.current = false;
					refs.playerEntityIdRef.current = character.id;
					if (!refs.entityMetaRef.current[character.id]) {
						refs.entityMetaRef.current[character.id] = {
							kind: 'character',
							path: character.path,
							name: 'Player',
							physicsEnabled: true,
							physicsType: 'dynamic',
						};
					} else {
						refs.entityMetaRef.current[character.id].physicsEnabled = true;
						refs.entityMetaRef.current[character.id].physicsType = 'dynamic';
					}
					const savedFpView = refs.pendingPlayCharacterViewRef.current
						?? refs.playCharacterViewRef.current;
					if (
						refs.sceneBurstLoadInProgressRef.current
						&& savedFpView?.position
					) {
						refs.sceneBurstAwaitingPlayerViewRef.current = true;
					}
					applyPendingRestore(character.id, character.path, {
						skipTransform: !!savedFpView?.position,
					});
					applySavedPlayCharacterView(savedFpView, { editorOrbit: true });
					if (savedFpView?.control_bindings) {
						const meta = refs.entityMetaRef.current[character.id];
						if (meta) {
							meta.controlBindings = savedFpView.control_bindings;
						}
						window.engine.send({
							cmd: 'set_control_bindings',
							id: character.id,
							bindings: savedFpView.control_bindings,
						} as never);
					}
					refs.pendingPlayCharacterViewRef.current = null;
					// No sobrescribir scale: el motor puede usar 1.0 tras importar modelo (malla ya normalizada).
					applyPlayCharacterDefaultsForPlayer(character.id, gameStyle, refs);
				} else {
					dispatch({ type: 'ADD_CHARACTER', payload: { id: character.id, path: character.path } });
					refs.entityMetaRef.current[character.id] = { kind: 'character', path: '[Player]', physicsEnabled: false, physicsType: '' };
					applyPendingRestore(character.id, character.path);
					const duplicateTransform = refs.pendingDupQ.current.shift();
					if (duplicateTransform) {
						window.engine.send({ cmd: 'set_transform', id: character.id, position: duplicateTransform.position, rotation: duplicateTransform.rotation, scale: duplicateTransform.scale, track_undo: false } as never);
						refs.entityTransformsRef.current[character.id] = duplicateTransform;
					}
				}
			} else {
				dispatch({ type: 'ADD_CHARACTER', payload: { id: character.id, path: character.path } });
				const existingMeta = refs.entityMetaRef.current[character.id];
				if (existingMeta) {
					refs.entityMetaRef.current[character.id] = { ...existingMeta };
				} else {
					refs.entityMetaRef.current[character.id] = { kind: 'character', path: character.path, physicsEnabled: false, physicsType: '' };
				}
				applyPendingRestore(character.id, character.path);
			}
			tryEndSceneBurstLoad(dispatch, refs.sceneBurstLoadInProgressRef, refs, reportBounds);
		}

		if (event.event === 'sprite_loaded') {
			if (refs.sceneImportInProgressRef.current) return;
			const sprite = event as unknown as { path: string; name: string; width: number; height: number };
			dispatch({ type: 'ADD_SPRITE', payload: { path: sprite.path, name: sprite.name, width: sprite.width, height: sprite.height } });
		}

		if (event.event === 'sprite_removed') {
			const sprite = event as unknown as SpriteRemoved;
			dispatch({ type: 'REMOVE_SPRITE', payload: sprite.path });
		}

		if (event.event === 'sprites_list') {
			const spritesList = event as unknown as SpritesList;
			dispatch({ type: 'SET_SPRITES', payload: spritesList.sprites });
		}

		if (event.event === 'model_asset_loaded') {
			const model = event as unknown as { path: string; name: string };
			dispatch({ type: 'ADD_MODEL_INFO', payload: { path: model.path, name: model.name } });
		}

		if (event.event === 'model_asset_removed') {
			const model = event as unknown as { path: string };
			dispatch({ type: 'REMOVE_MODEL_INFO', payload: model.path });
		}

		if (event.event === 'models_list') {
			const modelsList = event as unknown as { models: { path: string; name: string }[] };
			dispatch({ type: 'SET_MODELS', payload: modelsList.models });
		}

		if (event.event === 'sound_loaded') {
			const sound = event as unknown as { path: string; name: string };
			dispatch({ type: 'ADD_SOUND', payload: { path: sound.path, name: sound.name } });
		}

		if (event.event === 'sound_removed') {
			const sound = event as unknown as { path: string };
			dispatch({ type: 'REMOVE_SOUND', payload: sound.path });
		}

		if (event.event === 'sounds_list') {
			const soundsList = event as unknown as { sounds: { path: string; name: string }[] };
			dispatch({ type: 'SET_SOUNDS', payload: soundsList.sounds });
		}

		if (event.event === 'background_asset_loaded') {
			const bg = event as unknown as { path: string; name: string };
			dispatch({ type: 'ADD_BACKGROUND', payload: { path: bg.path, name: bg.name } });
		}

		if (event.event === 'background_asset_removed') {
			const bg = event as unknown as { path: string };
			dispatch({ type: 'REMOVE_BACKGROUND', payload: bg.path });
		}

		if (event.event === 'backgrounds_list') {
			const bgList = event as unknown as { backgrounds: { path: string; name: string }[] };
			dispatch({ type: 'SET_BACKGROUNDS', payload: bgList.backgrounds });
		}

		if (event.event === 'stopped') {
			if (refs.sceneImportInProgressRef.current) {
				endSceneImportLoading(
					dispatch,
					refs.sceneImportInProgressRef,
					refs.pendingImportSceneRef,
					reportBounds,
				);
			}
			if (refs.sceneBurstLoadInProgressRef.current) {
				refs.sceneBurstAwaitingPlayerViewRef.current = false;
				refs.sceneBurstPendingColliderCountRef.current = 0;
				endSceneBurstLoad(dispatch, refs.sceneBurstLoadInProgressRef, reportBounds);
			}
			dispatch({ type: 'ENGINE_STOPPED', payload: (event as { code?: number }).code });
		}

		if (
			event.event === 'play_character_view_changed'
			|| event.event === 'first_person_view_changed'
		) {
			const ev = event as unknown as PlayCharacterViewChangedEvent;
			applyPlayCharacterViewFromEngine(
				ev,
				refs.playCharacterViewRef,
				refs.entityTransformsRef,
				refs.playerEntityIdRef,
			);
			dispatch({ type: 'SYNC_PLAY_CHARACTER_VIEW' });
			if (refs.sceneBurstLoadInProgressRef.current) {
				refs.sceneBurstAwaitingPlayerViewRef.current = false;
				tryEndSceneBurstLoad(dispatch, refs.sceneBurstLoadInProgressRef, refs, reportBounds);
			}
		}

		if (event.event === 'preview_playing_changed') {
			const playing = Boolean((event as { playing?: boolean }).playing);
			dispatch({ type: 'SET_PREVIEW_PLAYING', payload: playing });
		}

		if (event.event === 'atlas_exhausted') {
			const e = event as { atlas_size?: number; width?: number; height?: number };
			const atlas = e.atlas_size ?? 4096;
			const w = e.width ?? 0;
			const h = e.height ?? 0;
			addLog(
				`[atlas] Texture atlas full (${atlas}×${atlas}). Could not pack ${w}×${h}. Remove sprites or change scene.`,
				true,
			);
		}

		if (event.event === 'error') {
			if (refs.sceneImportInProgressRef.current) {
				endSceneImportLoading(
					dispatch,
					refs.sceneImportInProgressRef,
					refs.pendingImportSceneRef,
					reportBounds,
				);
			}
			if (refs.sceneBurstLoadInProgressRef.current) {
				refs.sceneBurstAwaitingPlayerViewRef.current = false;
				refs.sceneBurstPendingColliderCountRef.current = 0;
				endSceneBurstLoad(dispatch, refs.sceneBurstLoadInProgressRef, reportBounds);
			}
			dispatch({ type: 'SET_ERROR', payload: (event as { message?: string }).message ?? 'Error desconocido' });
		}

		if (event.event === 'drawing_progress') {
			dispatch({ type: 'SET_TOOL_PROGRESS', payload: (event as { count?: number }).count ?? 0 });
		}

		if (event.event === 'collider_created') {
			if (refs.sceneImportInProgressRef.current) return;
			const collider = event as { id?: number; points?: [[number, number], [number, number], [number, number], [number, number]] };
			const id = collider.id ?? -1;
			refs.entityMetaRef.current[id] = { kind: 'collider', path: '[Colisionador]', physicsEnabled: true, physicsType: 'static', points: collider.points };
			const transformFromPoints = buildTransformFromPoints(collider.points);
			if (transformFromPoints) {
				refs.entityTransformsRef.current[id] = transformFromPoints;
			}
			dispatch({ type: 'ADD_COLLIDER', payload: { id, path: '[Colisionador]' } });
			dispatch({ type: 'SET_TOOL_PROGRESS', payload: null });
			if (refs.sceneBurstLoadInProgressRef.current) {
				refs.sceneBurstPendingColliderCountRef.current = Math.max(
					0,
					refs.sceneBurstPendingColliderCountRef.current - 1,
				);
				tryEndSceneBurstLoad(dispatch, refs.sceneBurstLoadInProgressRef, refs, reportBounds);
			}
		}

		if (event.event === 'execution_area_created') {
			if (refs.sceneImportInProgressRef.current) return;
			const area = event as { id?: number; points?: [[number, number], [number, number], [number, number], [number, number]] };
			const id = area.id ?? -1;
			refs.entityMetaRef.current[id] = { kind: 'execution_area', path: '[ExecutionArea]', physicsEnabled: false, physicsType: 'static', points: area.points };
			const transformFromPoints = buildTransformFromPoints(area.points);
			if (transformFromPoints) {
				refs.entityTransformsRef.current[id] = transformFromPoints;
			}
			const queue = refs.pendingRestoresRef.current.get('[ExecutionArea]');
			if (queue && queue.length > 0) {
				const pending = queue.shift()!;
				sendApplyEntityRestore(id, pending, { applyInitialAnimationFrame: false });
				applyPendingRestoreMeta(refs, id, pending);
				if (queue.length === 0) refs.pendingRestoresRef.current.delete('[ExecutionArea]');
			}
			dispatch({ type: 'ADD_EXECUTION_AREA', payload: { id, path: '[ExecutionArea]' } });
			dispatch({ type: 'SET_TOOL_PROGRESS', payload: null });
			tryEndSceneBurstLoad(dispatch, refs.sceneBurstLoadInProgressRef, refs, reportBounds);
		}

		if (event.event === 'tool_cancelled') {
			dispatch({ type: 'SET_TOOL_PROGRESS', payload: null });
		}

		if (event.event === 'pivot_selected') {
			const pivot = event as unknown as PivotSelected;
			refs.pivotEditListenerRef.current?.(pivot.frame_path, pivot.pivot_x, pivot.pivot_y);
		}

		if (event.event === 'quick_build_click') {
			const e = event as unknown as { x: number; y: number; fit_to_grid?: boolean; scale?: [number, number, number] };
			refs.quickBuildClickListenerRef.current?.(e.x, e.y, !!e.fit_to_grid, e.scale);
		}

		if (event.event === 'animation_logical_resolved') {
			const e = event as unknown as AnimationLogicalResolved;
			const meta = refs.entityMetaRef.current[e.id];
			if (!meta?.animations) return;
			const patchLogical = (anims: any[]) =>
				anims.map((anim: any) =>
					anim?.name === e.name
						? { ...anim, logical_w: e.logical_w, logical_h: e.logical_h }
						: anim,
				);
			meta.animations = patchLogical(meta.animations);
			const bpId = meta.blueprintId;
			if (bpId) {
				refs.blueprintsRef.current = refs.blueprintsRef.current.map((bp) =>
					bp.id === bpId && bp.animations
						? { ...bp, animations: patchLogical(bp.animations) }
						: bp,
				);
				dispatch({ type: 'SET_BLUEPRINTS', payload: refs.blueprintsRef.current });
			}
			dispatch({
				type: 'UPDATE_ENTITY_ANIMATIONS',
				payload: { entityId: e.id, animations: meta.animations },
			});

			const anim = meta.animations.find((a: { name?: string }) => a?.name === e.name);
			const frame = anim?.frames?.[0];
			if (anim && frame?.path) {
				window.engine.send(
					buildPlayAnimationFrameCmd(
						e.id,
						{ logical_w: e.logical_w, logical_h: e.logical_h },
						frame,
					) as never,
				);
			}
		}

		if (event.event === 'entity_removed') {
			const e = event as import('@shared-types').EntityRemoved;
			const removedKind = e.kind ?? refs.entityMetaRef.current[e.id]?.kind;
			if (
				e.points
				&& (removedKind === 'collider' || removedKind === 'execution_area')
				&& refs.entityMetaRef.current[e.id]
			) {
				refs.entityMetaRef.current[e.id].points = e.points;
			}
			dispatch({ type: 'REMOVE_ENTITY', payload: e.id });
			if (refs.playerEntityIdRef.current === e.id) refs.playerEntityIdRef.current = null;
			delete refs.entityMetaRef.current[e.id];
			delete refs.entityTransformsRef.current[e.id];
		}

		if (event.event === 'multi_select_changed') {
			const e = event as unknown as { ids: number[] };
			dispatch({ type: 'SET_MULTI_SELECT', payload: e.ids });
		}

		if (event.event === 'multi_selection_transformed') {
			const e = event as unknown as { entities: Array<{ id: number; position: [number, number, number]; rotation: [number, number, number, number]; scale: [number, number, number] }> };
			for (const entity of e.entities) {
				refs.entityTransformsRef.current[entity.id] = {
					position: entity.position,
					rotation: entity.rotation,
					scale: entity.scale,
				};
			}
		}

		if (event.event === 'animation_finished') {
			const animationFinished = event as unknown as AnimationFinished;
			const pending = refs.pendingEventsRef.current.get('animation_finished');
			if (pending) {
				pending.resolve(animationFinished);
				refs.pendingEventsRef.current.delete('animation_finished');
			}
			dispatch({ type: 'SET_ANIMATION_PLAYING', payload: { entityId: animationFinished.entity_id, playing: false } });
		}

		if (event.event === 'physics_changed') {
			const physicsChanged = event as unknown as PhysicsChanged;
			if (refs.entityMetaRef.current[physicsChanged.entity_id]) {
				refs.entityMetaRef.current[physicsChanged.entity_id].physicsEnabled = physicsChanged.enabled;
				refs.entityMetaRef.current[physicsChanged.entity_id].physicsType = physicsChanged.body_type;
			}
			dispatch({
				type: 'UPDATE_SELECTED_PHYSICS',
				payload: { entityId: physicsChanged.entity_id, enabled: physicsChanged.enabled, bodyType: physicsChanged.body_type },
			});
		}

		if (event.event === 'debug_metrics') {
			const metrics = event as unknown as DebugMetrics;
			dispatch({ type: 'SET_DEBUG_METRICS', payload: metrics });
			// Solo actualizar la vista guardada (pies/yaw/pitch). No reescribir entityTransformsRef
			// en play: eso desalineaba centro/pivot respecto al motor y el panel de Propiedades.
			if (
				gameStyle === 'first-person'
				&& projectType === '3D'
				&& (metrics.play_character_position ?? metrics.first_person_position)
				&& (metrics.play_character_yaw ?? metrics.first_person_yaw) != null
				&& (metrics.play_character_pitch ?? metrics.first_person_pitch) != null
			) {
				const prev = refs.playCharacterViewRef.current;
				refs.playCharacterViewRef.current = {
					position: (metrics.play_character_position ?? metrics.first_person_position)!,
					scale: prev?.scale ?? FIRST_PERSON_PLAYER_BODY_SCALE,
					yaw: (metrics.play_character_yaw ?? metrics.first_person_yaw)!,
					pitch: (metrics.play_character_pitch ?? metrics.first_person_pitch)!,
					...(prev?.visual_model_path ? { visual_model_path: prev.visual_model_path } : {}),
				};
			}
		}
	};
}