import type { Dispatch } from 'react';
import type { Locale } from '../../LanguageContext';
import type {
	AnimationLogicalResolved,
	AnimationFinished,
	Camera2dUpdated,
	CharacterLoaded,
	DebugMetrics,
	EntityRemoved,
	EntitySelected,
	PhysicsChanged,
	PivotSelected,
	ScenarioLoaded,
	SpriteRemoved,
	SpritesList,
} from '@shared-types';
import type {
	GameStyle,
	PlayCharacterViewChanged,
	ProjectLoaded2dPayload,
	ProjectLoaded3dPayload,
	SavedScene,
} from '@shared-types';
import {
	DEFAULT_LIGHT_AMBIENT,
	DEFAULT_LIGHT_INTENSITY,
	DEFAULT_SHADOW_DARKNESS,
	FIRST_PERSON_PLAYER_BODY_SCALE,
	isEditorBoxPath,
	isEditorCameraPath,
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
	savedPlayCharacterViewForRestore,
} from '../../../defaults/playCharacterSceneRestore';
import { buildSetSceneCommand } from '../../../defaults/projectSceneLoad';
import { engineSceneToSavedScene, requestEngineSaveSnapshot } from '../../../defaults/buildProjectSaveFromEngine';
import { isModel3DPath, is3dModelFileEntity } from '../../../utils/blueprintModelPath';
import {
	buildImportSceneCommand,
	is2dProjectLoadedByEngine,
	is3dProjectLoadedByEngine,
	resolveActiveSceneSave,
	resolveEntityTransform,
	resolveSavedEntityTransform,
	syncEditorStateFromSavedScene,
} from './buildImportSceneCommand';
import { applyPendingRestoreMeta, buildPlayAnimationFrameCmd, sendApplyEntityRestore } from './applyPendingRestoreToEngine';
import {
	beginSceneBurstLoad,
	beginSceneImportLoading,
	beginModelReplaceLoading,
	beginEngineBootEntityWait,
	completeEngineBootIpcEvent,
	endEngineBootLoadingIfIdle,
	isEngineBootScenePreloaded,
	trackEngineBootIpcSeen,
	tryFinishEngineBootLoading,
	endModelReplaceLoading,
	endSceneBurstLoad,
	endSceneImportLoading,
	needsSceneBurstLoad,
	trackSceneBurstCollider,
	trackSceneBurstOp,
	trackSceneBurstModelPreloads,
	completeSceneBurstOp,
	tryEndSceneBurstLoad,
	takePendingBurstSpawnRestoreForPath,
	isPlayCharacterVisualModelReplace,
	finishPlayCharacterBurstRestore,
	takePendingModelLoadByPath,
	takePendingRestoreByPath,
	drainPendingRestoreSlot,
	flushPendingCachedModelSpawnsForPath,
	collectUncachedBurstModelPaths,
	hasQueuedCachedModelSpawns,
} from './sceneImportOverlay';
import type { EngineAction, EngineInternalRefs, EntityMeta, PendingRestore, Transform } from '../types';

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
	'quick_build_click',
	'camera_2d_updated',
	'animation_logical_resolved',
	'multi_selection_transformed',
	'autosave_tick',
	'atlas_exhausted',
	'preview_playing_changed',
	'play_character_view_changed', 'first_person_view_changed',
	'save_snapshot_ready',
	'load_progress',
	'model_asset_preload_started',
	'model_asset_loaded',
]);

/** Eventos IPC que no deben duplicar `[Carga]` durante carga de escena o plantilla FP al arrancar. */
const SCENE_LOAD_SILENT_EVENTS = new Set<string>([
	'model_loaded',
	'model_clips_ready',
	'entity_model_replaced',
	'models_list',
	'sounds_list',
	'backgrounds_list',
	'default_scene_name_ready',
	'project_loaded_3d',
	'project_load_3d_complete',
]);

function shouldSilenceEngineEventLog(
	eventName: string,
	refs: Pick<
		EngineInternalRefs,
		| 'sceneImportInProgressRef'
		| 'engineBootAwaitRef'
		| 'engineBootFinishedRef'
		| 'initialSaveRef'
	>,
	projectType?: string,
): boolean {
	if (SILENT_ENGINE_EVENTS.has(eventName)) return true;
	const bootPreloaded = isEngineBootScenePreloaded(
		projectType,
		Boolean(refs.initialSaveRef.current),
	);
	const bootLogsActive = bootPreloaded && !refs.engineBootFinishedRef.current;
	const loadPanelActive =
		refs.sceneImportInProgressRef.current
		|| refs.engineBootAwaitRef.current
		|| bootLogsActive;
	if (loadPanelActive && SCENE_LOAD_SILENT_EVENTS.has(eventName)) return true;
	return false;
}

/** Línea del panel de logs; `null` = no imprimir; `undefined` = JSON del evento. */
function panelLogLineForEngineEvent(
	event: RuntimeEngineEvent,
	refs: Pick<
		EngineInternalRefs,
		| 'sceneImportInProgressRef'
		| 'engineBootAwaitRef'
		| 'engineBootFinishedRef'
		| 'initialSaveRef'
	>,
	projectType?: string,
): string | null | undefined {
	if (event.event === 'model_loaded') {
		const name = (event as { name?: string }).name;
		const bootPreloaded = isEngineBootScenePreloaded(
			projectType,
			Boolean(refs.initialSaveRef.current),
		);
		if (bootPreloaded && name?.startsWith('Sun')) {
			return '[Carga] Insertando Sol (Sun)';
		}
	}
	if (shouldSilenceEngineEventLog(event.event, refs, projectType)) return null;
	return undefined;
}

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
		if (event.event === 'load_progress') {
			const progress = event as {
				message?: string
				step_ms?: number
				total_ms?: number
			};
			const msg = progress.message?.trim() ?? '';
			if (msg) {
				const timing =
					progress.step_ms != null && progress.total_ms != null
						? ` (+${progress.step_ms} ms, total ${progress.total_ms} ms)`
						: '';
				addLog(`[Carga] ${msg}${timing}`);
			}
		}

		const panelLine = panelLogLineForEngineEvent(event, refs, projectType);
		if (panelLine != null) {
			addLog(panelLine, event.event === 'error');
		} else if (panelLine === undefined) {
			addLog(JSON.stringify(event), event.event === 'error');
		}

		const pendingEvent = refs.pendingEventsRef.current.get(event.event);
		if (pendingEvent) {
			pendingEvent.resolve(event);
			refs.pendingEventsRef.current.delete(event.event);
		}

		const sendEngine = window.engine.send;

		if (event.event === 'ready') {
			const engineLoads2dSave = is2dProjectLoadedByEngine(
				projectType,
				refs.initialExtractDirRef.current,
			);
			const engineLoads3dSave = is3dProjectLoadedByEngine(
				projectType,
				refs.initialExtractDirRef.current,
			);
			const engineLoadsSaveFromExtract = engineLoads2dSave || engineLoads3dSave;
			dispatch({ type: 'SET_READY' });
			dispatch({ type: 'SET_PREVIEW_PLAYING', payload: false });
			if (refs.readyTimer.current) clearTimeout(refs.readyTimer.current);
			const baseSave = refs.initialSaveRef.current;
			const boot3dNoSave =
				isEngineBootScenePreloaded(projectType, Boolean(baseSave)) && !engineLoads3dSave;
			// Igual que main: si el motor ya cargó la plantilla 3D al arrancar, no repetir set_scene.
			if (projectType && !engineLoadsSaveFromExtract && !boot3dNoSave) {
				window.engine.send(
					buildSetSceneCommand(projectType, refs.initialSavePathRef.current) as never,
				);
			} else if (boot3dNoSave) {
				beginEngineBootEntityWait(refs);
			}
			window.engine.send({ cmd: 'set_preview_playing', playing: false } as never);
			refs.mainPlayerHandled.current = false;
			refs.playerRemoved.current = false;
			refs.pendingPlayerDups.current = [];
			refs.pendingDupQ.current = [];
			let savedGravity: number | undefined;
			if (baseSave && !engineLoadsSaveFromExtract) {
				const { save, activeScene } = resolveActiveSceneSave(baseSave);
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
					if (!importScene2d && !engineLoads2dSave) {
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
				if (!importScene2d && !engineLoads2dSave) {
					sendEngine({ cmd: 'set_camera2d', x: save.camera2d.x, y: save.camera2d.y, half_h: save.camera2d.halfH } as never);
				}
			}
				if (engineLoads2dSave && save.sprites && save.sprites.length > 0) {
					dispatch({ type: 'SET_LOADED_SPRITES_INFO', payload: save.sprites });
				} else if (!importScene2d && save.sprites && save.sprites.length > 0) {
					for (const sprite of save.sprites) {
						sendEngine({ cmd: 'load_sprite', path: sprite.path, name: sprite.name } as never);
						dispatch({ type: 'ADD_SPRITE_INFO', payload: { path: sprite.path, name: sprite.name } });
					}
				}
				const burstLoadPlanned = !importScene2d
					&& needsSceneBurstLoad(projectType, gameStyle, save);
				if (save.models && save.models.length > 0 && !burstLoadPlanned) {
					for (const model of save.models) {
						sendEngine({ cmd: 'load_model_asset', path: model.path, name: model.name } as never);
						dispatch({ type: 'ADD_MODEL_INFO', payload: { path: model.path, name: model.name, loading: true } });
					}
				}
				if (save.sounds && save.sounds.length > 0) {
					if (!engineLoads2dSave) {
						for (const sound of save.sounds) {
							sendEngine({ cmd: 'load_sound', path: sound.path, name: sound.name } as never);
						}
					}
					dispatch({ type: 'SET_SOUNDS', payload: save.sounds });
				}
				if (save.backgrounds && save.backgrounds.length > 0) {
					if (!engineLoads2dSave) {
						for (const bg of save.backgrounds) {
							sendEngine({ cmd: 'load_background_asset', path: bg.path, name: bg.name } as never);
						}
					}
					dispatch({ type: 'SET_BACKGROUNDS', payload: save.backgrounds });
				}
				if (save.backgroundPath && !importScene2d && !engineLoads2dSave) {
					sendEngine({ cmd: 'load_background', path: save.backgroundPath } as never);
				}
				let burstLoad = false;
				if (importScene2d) {
					if (!engineLoads2dSave) {
						const sceneForImport: SavedScene = {
							id: activeScene?.id ?? 1,
							name: activeScene?.name ?? '',
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
					}
				} else if (!engineLoads2dSave && !engineLoads3dSave) {
				burstLoad = needsSceneBurstLoad(projectType, gameStyle, save);
				if (burstLoad) {
					refs.sceneBurstPendingColliderCountRef.current = 0;
					beginSceneBurstLoad(dispatch, refs.sceneBurstLoadInProgressRef, refs);
					if (save.models && save.models.length > 0) {
						trackSceneBurstModelPreloads(refs, save.models.length);
						for (const model of save.models) {
							sendEngine({ cmd: 'load_model_asset', path: model.path, name: model.name } as never);
							dispatch({ type: 'ADD_MODEL_INFO', payload: { path: model.path, name: model.name, loading: true } });
						}
					}
				}
				const loadBlueprints = save.blueprints ?? refs.blueprintsRef.current;
				for (const entity of save.entities) {
					const transform = is3dModelFileEntity(projectType, entity)
						? resolveSavedEntityTransform(entity)
						: resolveEntityTransform(entity, loadBlueprints);
					if (entity.kind === 'collider' && entity.points) {
						if (projectType === '2D') {
							if (burstLoad) {
								trackSceneBurstCollider(refs);
								trackSceneBurstOp(refs);
							}
							sendEngine({ cmd: 'create_collider_from_points', points: entity.points, track_undo: false } as never);
						}
					} else if (entity.kind === 'execution_area' && entity.points) {
						if (projectType === '2D') {
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
							if (burstLoad) trackSceneBurstOp(refs);
							sendEngine({ cmd: 'create_execution_area_from_points', points: entity.points, track_undo: false } as never);
						}
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
						if (burstLoad) trackSceneBurstOp(refs);
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
						if (burstLoad) trackSceneBurstOp(refs);
						sendEngine({
							cmd: 'spawn_sun',
							name: entity.name ?? '',
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
						if (burstLoad) trackSceneBurstOp(refs);
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
						if (burstLoad) trackSceneBurstOp(refs);
						sendEngine({
							cmd: 'spawn_editor_box',
							name: entity.name ?? '',
							position: entity.position,
							scale: entity.scale,
						} as never);
					} else if (entity.kind === 'scenario' && projectType === '3D' && !isModel3DPath(entity.path)) {
						// Escenarios 2D legacy sin archivo 3D.
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
						if (!burstLoad) {
							const queue = refs.pendingRestoresRef.current.get(entity.path) ?? [];
							queue.push(pendingRestore);
							refs.pendingRestoresRef.current.set(entity.path, queue);
						}
						if (entity.kind === 'scenario') sendEngine({ cmd: 'load_scenario', path: entity.path } as never);
						if (is3dModelFileEntity(projectType, entity)) {
							refs.pendingModelLoadQueueRef.current.push({
								modelPath: entity.path,
								pending: pendingRestore,
							});
							if (!burstLoad) {
								sendEngine({
									cmd: 'load_model',
									path: entity.path,
									single_instance: true,
									...(entity.entity_category
										? { entity_category: entity.entity_category }
										: {}),
								} as never);
							}
						}
					}
				}
				}
				if (burstLoad && refs.pendingModelLoadQueueRef.current.length > 0) {
					const preloadedPaths = (save.models ?? []).map((model) => model.path);
					const queuedPaths = refs.pendingModelLoadQueueRef.current.map((item) => item.modelPath);
					const extraPaths = collectUncachedBurstModelPaths(queuedPaths, preloadedPaths);
					if (extraPaths.size > 0) {
						trackSceneBurstModelPreloads(refs, extraPaths.size);
						for (const [path, name] of extraPaths) {
							sendEngine({ cmd: 'load_model_asset', path, name } as never);
							dispatch({ type: 'ADD_MODEL_INFO', payload: { path, name, loading: true } });
						}
					}
				}
				if (gameStyle === 'first-person' && projectType === '3D') {
					ensurePlayCharacterOnLoad(save, refs.pendingRestoresRef, (cmd) => sendEngine(cmd as never), {
						onBurstOp: burstLoad ? () => trackSceneBurstOp(refs) : undefined,
					});
				}
				if (burstLoad) {
					setTimeout(
						() => tryEndSceneBurstLoad(
							dispatch,
							refs.sceneBurstLoadInProgressRef,
							refs,
							refs.sceneImportInProgressRef,
							refs.modelReplaceInProgressRef,
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
			if (engineLoads3dSave) {
				beginSceneImportLoading(dispatch, refs.sceneImportInProgressRef);
			} else if (boot3dNoSave) {
				// Dejar procesar IPC pendientes (p. ej. Sun_01) antes de cerrar el boot.
				queueMicrotask(() => {
					tryFinishEngineBootLoading(dispatch, refs, reportBounds);
				});
			} else {
				endEngineBootLoadingIfIdle(dispatch, refs, reportBounds);
			}
		}

		if (event.event === 'model_loaded') {
			trackEngineBootIpcSeen(refs, projectType, Boolean(refs.initialSaveRef.current));
			const loaded = event as {
				id?: number
				name?: string
				position?: [number, number, number]
				scale?: [number, number, number]
				rotation?: [number, number, number, number]
				path?: string
				kind?: string
				blueprint_id?: string
				physics_enabled?: boolean
				physics_type?: string
				entity_category?: EntityCategory
			};
			const id = loaded.id ?? -1;
			dispatch({ type: 'ADD_ENTITY', payload: id });

			const burstPendingSpawn =
				refs.sceneBurstLoadInProgressRef.current
				&& loaded.path
					? takePendingBurstSpawnRestoreForPath(
						refs.pendingBurstSpawnRestoreRef.current,
						loaded.path,
					)
					: null;
			if (burstPendingSpawn) {
				const pending = burstPendingSpawn;
				const kind = (loaded.kind ?? 'model') as EntityMeta['kind'];
				const isEnvironment = pending.entityCategory === 'environment';
				refs.entityMetaRef.current[id] = {
					kind,
					path: loaded.path ?? pending.visualModelPath ?? '',
					name: pending.name ?? loaded.name ?? `Entity ${id}`,
					physicsEnabled: isEnvironment
						? true
						: (pending.physicsEnabled ?? loaded.physics_enabled ?? false),
					physicsType: isEnvironment
						? 'static'
						: (pending.physicsType ?? loaded.physics_type ?? 'static'),
					...(pending.entityCategory || loaded.entity_category
						? { entityCategory: pending.entityCategory ?? loaded.entity_category }
						: {}),
					...(pending.blueprintId || loaded.blueprint_id
						? { blueprintId: pending.blueprintId ?? loaded.blueprint_id }
						: {}),
				};
				if (loaded.position && loaded.scale) {
					refs.entityTransformsRef.current[id] = {
						position: loaded.position,
						rotation: loaded.rotation ?? pending.transform?.rotation ?? [0, 0, 0, 1],
						scale: loaded.scale,
					};
				}
				sendApplyEntityRestore(id, pending, {
					skipTransform: true,
					applyInitialAnimationFrame: true,
				});
				applyPendingRestoreMeta(refs, id, pending);
				completeSceneBurstOp(refs);
				tryEndSceneBurstLoad(
					dispatch,
					refs.sceneBurstLoadInProgressRef,
					refs,
					refs.sceneImportInProgressRef,
					refs.modelReplaceInProgressRef,
					reportBounds,
				);
				return;
			}

			if (loaded.blueprint_id && !refs.sceneBurstLoadInProgressRef.current) {
				const kind = (loaded.kind ?? 'model') as EntityMeta['kind'];
				refs.entityMetaRef.current[id] = {
					kind,
					path: loaded.path ?? '',
					name: loaded.name ?? `Entity ${id}`,
					physicsEnabled: loaded.physics_enabled ?? false,
					physicsType: loaded.physics_type ?? 'static',
					...(loaded.entity_category ? { entityCategory: loaded.entity_category } : {}),
					blueprintId: loaded.blueprint_id,
				};
				if (loaded.position && loaded.scale) {
					refs.entityTransformsRef.current[id] = {
						position: loaded.position,
						rotation: loaded.rotation ?? [0, 0, 0, 1],
						scale: loaded.scale,
					};
				}
				addLog(
					`[quick_build] entidad colocada: ${loaded.name ?? id} (id=${id})`,
				);
				tryEndSceneBurstLoad(
					dispatch,
					refs.sceneBurstLoadInProgressRef,
					refs,
					refs.sceneImportInProgressRef,
					refs.modelReplaceInProgressRef,
					reportBounds,
				);
				return;
			}

			if (loaded.position && loaded.scale) {
				refs.entityTransformsRef.current[id] = {
					position: loaded.position,
					rotation: [0, 0, 0, 1],
					scale: loaded.scale,
				};
			}
			let burstHandled = false;
			let burstDeferComplete = false;
			const burstActive = refs.sceneBurstLoadInProgressRef.current;
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
				burstHandled = true;
			} else {
			const groundQueue = refs.pendingRestoresRef.current.get('[Ground]');
			if (groundQueue && groundQueue.length > 0) {
				const pending = groundQueue.shift()!;
				refs.entityMetaRef.current[id] = {
					kind: 'model',
					path: '[Ground]',
					name: pending.name ?? loaded.name ?? '',
					physicsEnabled: false,
					physicsType: 'static',
				};
				sendApplyEntityRestore(id, pending, {
					skipTransform: true,
					applyInitialAnimationFrame: false,
				});
				applyPendingRestoreMeta(refs, id, pending);
				if (groundQueue.length === 0) refs.pendingRestoresRef.current.delete('[Ground]');
				burstHandled = true;
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
				burstHandled = true;
			} else {
				// Fallback legacy: solo cuando no hay restores pendientes de save-load.
				// Evita clasificar como [Ground] cajas/muros llamados "Ground".
				if (
					!burstActive
					&& loaded.name?.toLowerCase() === 'ground'
					&& !refs.entityMetaRef.current[id]
				) {
					refs.entityMetaRef.current[id] = {
						kind: 'model',
						path: '[Ground]',
						name: 'Ground',
						physicsEnabled: false,
						physicsType: 'static',
					};
				}
				const spawnModelPath = refs.pendingModelPathRef.current;
				const spawnKind = refs.pendingSpawnKindRef.current ?? 'model';
				const spawnCategory = refs.pendingSpawnCategoryRef.current;
				let loadItem: { modelPath: string; pending: PendingRestore } | null = null;
				if (!spawnModelPath) {
					if (burstActive && loaded.path) {
						loadItem = takePendingModelLoadByPath(
							refs.pendingModelLoadQueueRef.current,
							loaded.path,
						);
					} else if (!burstActive) {
						loadItem = refs.pendingModelLoadQueueRef.current.shift() ?? null;
					}
				}
				let modelPath = spawnModelPath ?? loadItem?.modelPath ?? null;
				let restorePending = loadItem?.pending;
				if (!restorePending && modelPath) {
					let qbQueue = refs.pendingRestoresRef.current.get(modelPath);
					if ((!qbQueue || qbQueue.length === 0) && modelPath) {
						const base = modelPath.split(/[/\\]/).pop()?.toLowerCase();
						if (base) {
							for (const [key, queue] of refs.pendingRestoresRef.current.entries()) {
								if (queue.length > 0 && key.split(/[/\\]/).pop()?.toLowerCase() === base) {
									qbQueue = queue;
									modelPath = key;
									break;
								}
							}
						}
					}
					if (qbQueue && qbQueue.length > 0) {
						restorePending = qbQueue.shift()!;
						if (qbQueue.length === 0) {
							refs.pendingRestoresRef.current.delete(modelPath);
						}
					}
				}
				if (!restorePending && burstActive && loaded.path) {
					const matched = takePendingRestoreByPath(
						refs.pendingRestoresRef.current,
						loaded.path,
					);
					if (matched) {
						restorePending = matched.pending;
						modelPath = modelPath ?? matched.path;
					}
				}
				if (!restorePending && burstActive && loaded.name) {
					for (const [key, queue] of refs.pendingRestoresRef.current.entries()) {
						if (queue.length === 0 || key.startsWith('[')) continue;
						const head = queue[0];
						if (head.name && head.name === loaded.name) {
							restorePending = queue.shift()!;
							modelPath = modelPath ?? key;
							if (queue.length === 0) refs.pendingRestoresRef.current.delete(key);
							break;
						}
					}
				}
				if (!restorePending && spawnModelPath && !burstActive) {
					for (const [key, queue] of refs.pendingRestoresRef.current.entries()) {
						if (queue.length > 0) {
							restorePending = queue.shift()!;
							modelPath = modelPath ?? key;
							if (queue.length === 0) {
								refs.pendingRestoresRef.current.delete(key);
							}
							break;
						}
					}
				}

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
					const pendingVisualReplace = Boolean(
						visualPath && visualPath !== modelPath,
					);
					if (pendingVisualReplace) {
						burstDeferComplete = true;
						window.engine.send({
							cmd: 'replace_entity_model',
							id,
							path: visualPath,
						} as never);
					}
					refs.pendingModelPathRef.current = null;
					refs.pendingSpawnKindRef.current = null;
					refs.pendingSpawnCategoryRef.current = null;
					if (burstActive && modelPath) {
						drainPendingRestoreSlot(
							refs.pendingRestoresRef.current,
							refs.pendingModelLoadQueueRef.current,
							modelPath,
						);
					}
					burstHandled = true;
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
			if (burstActive && burstHandled && !burstDeferComplete) {
				completeSceneBurstOp(refs);
			}
			completeEngineBootIpcEvent(dispatch, refs, reportBounds);
			tryEndSceneBurstLoad(
				dispatch,
				refs.sceneBurstLoadInProgressRef,
				refs,
				refs.sceneImportInProgressRef,
				refs.modelReplaceInProgressRef,
				reportBounds,
			);
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
				if (isModel3DPath(replaced.path)) {
					refs.entityMetaRef.current[id].path = replaced.path;
				}
			}
			const meta = refs.entityMetaRef.current[id];
			if (meta && replaced.path) {
				dispatch({
					type: 'UPDATE_ENTITY_ANIMATIONS',
					payload: {
						entityId: id,
						animations: meta.animations ?? [],
						visualModelPath: replaced.path,
					},
				});
			}
			if (replaced.position && replaced.scale) {
				refs.entityTransformsRef.current[id] = {
					position: replaced.position,
					rotation: replaced.rotation ?? refs.entityTransformsRef.current[id]?.rotation ?? [0, 0, 0, 1],
					scale: replaced.scale,
				};
			}
			const isPlayerVisual = isPlayCharacterVisualModelReplace(refs, id, replaced.path);
			if (isPlayerVisual) {
				refs.playerEntityIdRef.current = id;
				if (!refs.entityMetaRef.current[id]) {
					const playerPending = refs.pendingRestoresRef.current.get('[Player]')?.[0];
					refs.entityMetaRef.current[id] = {
						kind: 'character',
						path: '[Player]',
						name: 'Player',
						physicsEnabled: true,
						physicsType: 'dynamic',
						controlBindings: playerPending?.controlBindings,
						visualModelPath: replaced.path ?? playerPending?.visualModelPath,
					};
				}
				const metaAfter = refs.entityMetaRef.current[id];
				if (metaAfter) {
					metaAfter.physicsEnabled = true;
					metaAfter.physicsType = 'dynamic';
					if (metaAfter.controlBindings) {
						window.engine.send({
							cmd: 'set_control_bindings',
							id,
							bindings: metaAfter.controlBindings,
						} as never);
					}
				}
				const savedView = savedPlayCharacterViewForRestore(
					refs.pendingPlayCharacterViewRef.current,
					refs.playCharacterViewRef.current,
				);
				if (savedView?.position) {
					applySavedPlayCharacterView(savedView);
				}
				if (refs.sceneBurstLoadInProgressRef.current) {
					finishPlayCharacterBurstRestore(refs);
				}
			} else if (refs.sceneBurstLoadInProgressRef.current) {
				completeSceneBurstOp(refs);
			}
			if (refs.sceneBurstLoadInProgressRef.current) {
				tryEndSceneBurstLoad(
					dispatch,
					refs.sceneBurstLoadInProgressRef,
					refs,
					refs.sceneImportInProgressRef,
					refs.modelReplaceInProgressRef,
					reportBounds,
				);
			}
			if (refs.modelReplaceInProgressRef.current) {
				endModelReplaceLoading(
					dispatch,
					refs.modelReplaceInProgressRef,
					refs.sceneImportInProgressRef,
					refs.sceneBurstLoadInProgressRef,
					reportBounds,
					refs.modelLoadOverlayKindRef,
				);
			}
		}

		if (event.event === 'entity_selected') {
			const selected = event as unknown as EntitySelected;
			let meta = refs.entityMetaRef.current[selected.id];
			if (selected.blueprint_id) {
				if (!meta) {
					refs.entityMetaRef.current[selected.id] = {
						kind: 'model',
						path: '',
						name: selected.name,
						physicsEnabled: selected.physics_enabled ?? false,
						physicsType: selected.physics_type ?? 'static',
						blueprintId: selected.blueprint_id,
					};
					meta = refs.entityMetaRef.current[selected.id];
				} else {
					meta.blueprintId = selected.blueprint_id;
				}
			}
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
					visualModelPath: meta?.visualModelPath,
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

		if (event.event === 'project_loaded_2d') {
			const payload = event as unknown as ProjectLoaded2dPayload;
			refs.projectLoaded2dMetaRef.current = payload;
			refs.blueprintsRef.current = payload.blueprints;
			if (payload.camera2d) {
				refs.camera2dRef.current = payload.camera2d;
			}
			if (payload.language === 'en' || payload.language === 'es') {
				setLocale?.(payload.language as Locale);
			}
			dispatch({ type: 'APPLY_PROJECT_LOADED_2D', payload });
			if (payload.entityCount > 0) {
				beginSceneImportLoading(dispatch, refs.sceneImportInProgressRef);
			}
			window.engine.send({ cmd: 'get_sprites_list' } as never);
			window.engine.send({ cmd: 'get_sounds_list' } as never);
			window.engine.send({ cmd: 'get_backgrounds_list' } as never);
			return;
		}

		if (event.event === 'project_loaded_3d') {
			const payload = event as unknown as ProjectLoaded3dPayload;
			refs.projectLoaded3dMetaRef.current = payload;
			refs.blueprintsRef.current = payload.blueprints;
			if (payload.playerTransform) {
				refs.pendingPlayCharacterViewRef.current = payload.playerTransform;
				refs.playCharacterViewRef.current = payload.playerTransform;
			}
			if (payload.language === 'en' || payload.language === 'es') {
				setLocale?.(payload.language as Locale);
			}
			dispatch({ type: 'APPLY_PROJECT_LOADED_3D', payload });
			if (
				(payload.entityCount > 0 || payload.playerTransform)
				&& !refs.sceneImportInProgressRef.current
			) {
				beginSceneImportLoading(dispatch, refs.sceneImportInProgressRef);
			}
			window.engine.send({ cmd: 'get_models_list' } as never);
			window.engine.send({ cmd: 'get_sounds_list' } as never);
			window.engine.send({ cmd: 'get_backgrounds_list' } as never);
			return;
		}

		if (event.event === 'project_load_3d_complete') {
			const engineLoads3dSave = is3dProjectLoadedByEngine(
				projectType,
				refs.initialExtractDirRef.current,
			);
			if (!engineLoads3dSave) return;
			const meta = refs.projectLoaded3dMetaRef.current;
			void (async () => {
				try {
					const snapshot = await requestEngineSaveSnapshot();
					const scene = engineSceneToSavedScene(
						snapshot,
						meta?.activeSceneId ?? 1,
						meta?.sceneName ?? '',
						refs.entityMetaRef.current,
					);
					syncEditorStateFromSavedScene(
						scene,
						refs,
						dispatch,
						refs.blueprintsRef.current,
					);
					if (scene.models?.length) {
						for (const model of scene.models) {
							dispatch({
								type: 'SYNC_MODEL_PRELOAD',
								payload: { path: model.path, name: model.name },
							});
						}
					}
					dispatch({ type: 'SYNC_PLAY_CHARACTER_VIEW' });
				} catch (err) {
					console.error('[project_load_3d_complete] sync desde snapshot del motor:', err);
				} finally {
					endSceneImportLoading(
						dispatch,
						refs.sceneImportInProgressRef,
						refs.pendingImportSceneRef,
						refs.sceneBurstLoadInProgressRef,
						refs.modelReplaceInProgressRef,
						reportBounds,
					);
				}
			})();
			return;
		}

		if (event.event === 'scene_imported') {
			const engineLoads2dSave = is2dProjectLoadedByEngine(
				projectType,
				refs.initialExtractDirRef.current,
			);
			if (engineLoads2dSave) {
				const meta = refs.projectLoaded2dMetaRef.current;
				void (async () => {
					try {
						const snapshot = await requestEngineSaveSnapshot();
						const scene = engineSceneToSavedScene(
							snapshot,
							meta?.activeSceneId ?? 1,
							meta?.sceneName ?? '',
							refs.entityMetaRef.current,
						);
						syncEditorStateFromSavedScene(
							scene,
							refs,
							dispatch,
							refs.blueprintsRef.current,
						);
						if (scene.sprites?.length) {
							dispatch({ type: 'SET_LOADED_SPRITES_INFO', payload: scene.sprites });
						}
						dispatch({ type: 'SYNC_PLAY_CHARACTER_VIEW' });
					} catch (err) {
						console.error('[project_loaded_2d] sync desde snapshot del motor:', err);
					} finally {
						endSceneImportLoading(
							dispatch,
							refs.sceneImportInProgressRef,
							refs.pendingImportSceneRef,
							refs.sceneBurstLoadInProgressRef,
							refs.modelReplaceInProgressRef,
							reportBounds,
						);
					}
				})();
				return;
			}

			let scene = refs.pendingImportSceneRef.current;
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
				refs.sceneBurstLoadInProgressRef,
				refs.modelReplaceInProgressRef,
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
			tryEndSceneBurstLoad(
				dispatch,
				refs.sceneBurstLoadInProgressRef,
				refs,
				refs.sceneImportInProgressRef,
				refs.modelReplaceInProgressRef,
				reportBounds,
			);
		}

		if (event.event === 'character_loaded') {
			if (refs.sceneImportInProgressRef.current) return;
			trackEngineBootIpcSeen(refs, projectType, Boolean(refs.initialSaveRef.current));
			const character = event as unknown as CharacterLoaded;
			const applyPendingRestore = (
				id: number,
				path: string,
				options?: { skipTransform?: boolean },
			) => {
				const queue = refs.pendingRestoresRef.current.get(path);
				if (!queue || queue.length === 0) return false;

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
				}

				if (queue.length === 0) refs.pendingRestoresRef.current.delete(path);
				return Boolean(pending.visualModelPath);
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
					const savedFpView = savedPlayCharacterViewForRestore(
						refs.pendingPlayCharacterViewRef.current,
						refs.playCharacterViewRef.current,
					);
					const awaitingVisualReplace = applyPendingRestore(character.id, character.path, {
						skipTransform: Boolean(savedFpView?.position),
						omitScale: !savedFpView?.body_scale,
					});
					if (!awaitingVisualReplace) {
						applySavedPlayCharacterView(savedFpView);
						if (refs.sceneBurstLoadInProgressRef.current) {
							finishPlayCharacterBurstRestore(refs);
						} else {
							refs.pendingPlayCharacterViewRef.current = null;
						}
					} else if (savedFpView) {
						refs.pendingPlayCharacterViewRef.current = savedFpView;
					}
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
			} else if (isEditorCameraPath(character.path)) {
				refs.editorCameraEntityIdRef.current = character.id;
				dispatch({ type: 'ADD_CHARACTER', payload: { id: character.id, path: character.path } });
				refs.entityMetaRef.current[character.id] = {
					kind: 'character',
					path: character.path,
					name: 'EditorCamera',
					physicsEnabled: false,
					physicsType: '',
				};
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
			completeEngineBootIpcEvent(dispatch, refs, reportBounds);
			tryEndSceneBurstLoad(
				dispatch,
				refs.sceneBurstLoadInProgressRef,
				refs,
				refs.sceneImportInProgressRef,
				refs.modelReplaceInProgressRef,
				reportBounds,
			);
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

		if (event.event === 'model_asset_preload_started') {
			const model = event as unknown as { path: string; name: string };
			dispatch({ type: 'SYNC_MODEL_PRELOAD', payload: { path: model.path, name: model.name } });
		}

		if (event.event === 'model_asset_loaded') {
			const model = event as unknown as { path: string; name: string };
			dispatch({ type: 'MARK_MODEL_READY', payload: { path: model.path, name: model.name } });
			const burstActive = refs.sceneBurstLoadInProgressRef.current;
			const hasQueuedSpawns = hasQueuedCachedModelSpawns(
				refs.pendingModelLoadQueueRef.current,
				model.path,
			);
			if (burstActive) {
				completeSceneBurstOp(refs);
			}
			if (hasQueuedSpawns) {
				flushPendingCachedModelSpawnsForPath(
					model.path,
					refs.pendingModelLoadQueueRef.current,
					(cmd) => sendEngine(cmd as never),
					refs,
					burstActive,
				);
			}
			if (burstActive) {
				tryEndSceneBurstLoad(
					dispatch,
					refs.sceneBurstLoadInProgressRef,
					refs,
					refs.sceneImportInProgressRef,
					refs.modelReplaceInProgressRef,
					reportBounds,
				);
			}
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
					refs.sceneBurstLoadInProgressRef,
					refs.modelReplaceInProgressRef,
					reportBounds,
				);
			}
			if (refs.modelReplaceInProgressRef.current) {
				refs.modelAssetPreloadPendingRef.current = 0;
				endModelReplaceLoading(
					dispatch,
					refs.modelReplaceInProgressRef,
					refs.sceneImportInProgressRef,
					refs.sceneBurstLoadInProgressRef,
					reportBounds,
					refs.modelLoadOverlayKindRef,
				);
			}
			if (refs.sceneBurstLoadInProgressRef.current) {
				refs.sceneBurstPendingColliderCountRef.current = 0;
				refs.sceneBurstPendingOpsRef.current = 0;
				endSceneBurstLoad(
					dispatch,
					refs.sceneBurstLoadInProgressRef,
					refs.sceneImportInProgressRef,
					refs.modelReplaceInProgressRef,
					reportBounds,
				);
			}
			dispatch({ type: 'ENGINE_STOPPED', payload: (event as { code?: number }).code });
		}

		if (
			event.event === 'play_character_view_changed'
			|| event.event === 'first_person_view_changed'
		) {
			const ev = event as unknown as PlayCharacterViewChanged;
			applyPlayCharacterViewFromEngine(
				ev,
				refs.playCharacterViewRef,
				refs.entityTransformsRef,
				refs.playerEntityIdRef,
				refs.editorCameraEntityIdRef,
				refs.pendingPlayCharacterViewRef.current?.body_rotation,
			);
			const refreshId = ev.player_id ?? ev.editor_camera_id;
			if (refreshId != null) {
				const tr = refs.entityTransformsRef.current[refreshId];
				if (tr) {
					dispatch({
						type: 'UPDATE_SELECTED_TRANSFORM',
						payload: {
							entityId: refreshId,
							position: tr.position,
							rotation: tr.rotation,
							scale: tr.scale,
						},
					});
				}
			}
			dispatch({ type: 'SYNC_PLAY_CHARACTER_VIEW' });
			if (refs.sceneBurstLoadInProgressRef.current) {
				tryEndSceneBurstLoad(
				dispatch,
				refs.sceneBurstLoadInProgressRef,
				refs,
				refs.sceneImportInProgressRef,
				refs.modelReplaceInProgressRef,
				reportBounds,
			);
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
					refs.sceneBurstLoadInProgressRef,
					refs.modelReplaceInProgressRef,
					reportBounds,
				);
			} else if (refs.modelReplaceInProgressRef.current) {
				refs.modelAssetPreloadPendingRef.current = 0;
				endModelReplaceLoading(
					dispatch,
					refs.modelReplaceInProgressRef,
					refs.sceneImportInProgressRef,
					refs.sceneBurstLoadInProgressRef,
					reportBounds,
					refs.modelLoadOverlayKindRef,
				);
			}
			if (refs.sceneBurstLoadInProgressRef.current) {
				refs.sceneBurstPendingColliderCountRef.current = 0;
				refs.sceneBurstPendingOpsRef.current = 0;
				endSceneBurstLoad(
					dispatch,
					refs.sceneBurstLoadInProgressRef,
					refs.sceneImportInProgressRef,
					refs.modelReplaceInProgressRef,
					reportBounds,
				);
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
				tryEndSceneBurstLoad(
				dispatch,
				refs.sceneBurstLoadInProgressRef,
				refs,
				refs.sceneImportInProgressRef,
				refs.modelReplaceInProgressRef,
				reportBounds,
			);
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
			tryEndSceneBurstLoad(
				dispatch,
				refs.sceneBurstLoadInProgressRef,
				refs,
				refs.sceneImportInProgressRef,
				refs.modelReplaceInProgressRef,
				reportBounds,
			);
		}

		if (event.event === 'tool_cancelled') {
			dispatch({ type: 'SET_TOOL_PROGRESS', payload: null });
			if (refs.modelReplaceInProgressRef.current) {
				endModelReplaceLoading(
					dispatch,
					refs.modelReplaceInProgressRef,
					refs.sceneImportInProgressRef,
					refs.sceneBurstLoadInProgressRef,
					reportBounds,
					refs.modelLoadOverlayKindRef,
				);
			}
		}

		if (event.event === 'pivot_selected') {
			const pivot = event as unknown as PivotSelected;
			refs.pivotEditListenerRef.current?.(pivot.frame_path, pivot.pivot_x, pivot.pivot_y);
		}

		if (event.event === 'quick_build_click') {
			const e = event as unknown as {
				x: number
				y: number
				z?: number
				fit_to_grid?: boolean
				scale?: [number, number, number]
			};
			refs.quickBuildClickListenerRef.current?.(
				e.x,
				e.y,
				e.z ?? 0,
				!!e.fit_to_grid,
				e.scale,
			);
		}

		if (event.event === 'quick_build_ghost_ready') {
			const ghost = event as { path?: string; name?: string };
			addLog(
				`[quick_build] herramienta activa${ghost.name ? `: ${ghost.name}` : ''}${ghost.path ? ` (${ghost.path.split(/[/\\]/).pop()})` : ''}`,
			);
			if (refs.modelReplaceInProgressRef.current) {
				endModelReplaceLoading(
					dispatch,
					refs.modelReplaceInProgressRef,
					refs.sceneImportInProgressRef,
					refs.sceneBurstLoadInProgressRef,
					reportBounds,
					refs.modelLoadOverlayKindRef,
				);
			}
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
			const e = event as unknown as EntityRemoved;
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

		if (event.event === 'model_clips_ready') {
			const clipsEvent = event as unknown as {
				id: number
				path: string
				clips: Array<{ name: string; duration_s: number; fps: number }>
			};
			const id = clipsEvent.id;
			const prevMeta = refs.entityMetaRef.current[id];
			const pathChanged = prevMeta?.visualModelPath !== clipsEvent.path;
			const hadEmbedded = prevMeta?.animations?.some((a) => a.embedded_in_model) ?? false;
			const prevDefault =
				pathChanged || !hadEmbedded
					? undefined
					: prevMeta?.animations?.find((a) => a.is_default)?.name;
			const mapped = clipsEvent.clips.map((c) => ({
				name: c.name,
				fps: Math.max(1, Math.round(c.fps)),
				loop: true,
				embedded_in_model: true as const,
				logical_w: 1,
				logical_h: 1,
				frames: [] as { path: string; pivot_x: number; pivot_y: number }[],
				is_default: prevDefault === c.name ? true : undefined,
			}));
			if (!mapped.some((a) => a.is_default) && mapped.length > 0) {
				mapped[0].is_default = true;
			}
			if (prevMeta) {
				refs.entityMetaRef.current[id] = {
					...prevMeta,
					animations: mapped,
					visualModelPath: clipsEvent.path,
				};
			} else {
				refs.entityMetaRef.current[id] = {
					kind: 'model',
					path: clipsEvent.path,
					name: `Entity ${id}`,
					physicsEnabled: true,
					physicsType: 'static',
					animations: mapped,
					visualModelPath: clipsEvent.path,
				};
			}
			const defaultClip = mapped.find((a) => a.is_default)?.name ?? mapped[0]?.name;
			if (defaultClip) {
				sendEngine({ cmd: 'set_default_animation', id, name: defaultClip } as never);
			}
			dispatch({
				type: 'UPDATE_ENTITY_ANIMATIONS',
				payload: {
					entityId: id,
					animations: mapped,
					visualModelPath: clipsEvent.path,
				},
			});
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