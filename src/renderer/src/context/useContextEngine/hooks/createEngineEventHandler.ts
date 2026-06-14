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
	activeEntityPropertiesHandlerRef,
	pushEntityPropertiesPatch,
} from '../../../modal-electron/entityPropertiesModalSessions';
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
	savedPlayCharacterViewForRestore,
} from '../../../defaults/playCharacterSceneRestore';
import { buildSetSceneCommand } from '../../../defaults/projectSceneLoad';
import { engineSceneToSavedScene, requestEngineSaveSnapshot } from '../../../defaults/buildProjectSaveFromEngine';
import {
	blueprintEntityCategoryForEngine,
	blueprintFromSave,
	buildQuickBuildPendingRestore,
	blueprintPlacementCategory,
	blueprintPlacementPhysics,
	inferEntity3dCategoryFromName,
	normalizeBlueprintCategory,
	reconcileCategoryWithName,
	isModel3DPath,
	is3dModelFileEntity,
} from '../../../utils/blueprintModelPath';
import { playViewFromPlayerAndCamera } from '../../../utils/entity3dEditorSync';
import {
	buildImportSceneCommand,
	is2dProjectLoadedByEngine,
	is3dProjectLoadedByEngine,
	isProjectOpenedFromSave,
	resolveEntityTransform,
	resolveSavedEntityTransform,
	syncEditorStateFromSavedScene,
	syncPlayerEntityMetaFromPlayer,
	syncPlayerEntityMetaFromTransform,
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
	endFpSceneBaselineLogging,
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
	hasQueuedCachedModelSpawns,
	pathsMatchForBurstRestore,
} from './sceneImportOverlay';
import {
	DEFAULT_PLAYER_UI_BUTTON_CONFIG,
} from '../../../pages/EngineView/components/sidebar/UIAccordion/components/playerUiButtonModel';
import type { EngineAction, EngineInternalRefs, EntityMeta, PendingRestore, Transform } from '../types';
import { buildEditingUiElementsFromEngineList } from '../types';
import { takePendingPlayerUiButtonConfig } from './createEngineActions';

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
	'sprites_list',
	'background_loaded',
	'sound_loaded',
	'sounds_list',
	'font_loaded',
	'fonts_list',
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
	'player_ui_text_box_added',
	'player_ui_text_box_updated',
	'player_ui_text_box_removed',
	'player_ui_text_boxes_list',
	'player_ui_button_added',
	'player_ui_button_removed',
	'player_ui_image_added',
	'player_ui_object_added',
	'player_ui_object_draw_ended',
	'player_ui_object_removed',
	'player_ui_image_removed',
	'player_ui_active_screen_changed',
	'hud_image_loaded',
	'hud_image_removed',
	'hud_images_list',
	'backgrounds_list',
	'models_list',
	'entity_removed',
	'play_character_view_changed', 'first_person_view_changed',
	'save_snapshot_ready',
	'load_progress',
	'model_asset_preload_started',
	'model_asset_importing',
	'model_asset_imported',
	'model_asset_loaded',
	'model_asset_load_failed',
	'plane_tool_ready',
	'tool_cancelled',
	'trigger_exited',
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
	'project_loaded_2d',
	'project_loaded_3d',
	'project_load_2d_complete',
	'project_load_3d_complete',
]);

function shouldSilenceEngineEventLog(
	eventName: string,
	refs: Pick<
		EngineInternalRefs,
		| 'sceneImportInProgressRef'
		| 'sceneBurstLoadInProgressRef'
		| 'engineBootAwaitRef'
		| 'engineBootFinishedRef'
		| 'initialExtractDirRef'
	>,
	projectType?: string,
): boolean {
	if (SILENT_ENGINE_EVENTS.has(eventName)) return true;
	const bootPreloaded = isEngineBootScenePreloaded(
		projectType,
		isProjectOpenedFromSave(refs.initialExtractDirRef.current),
	);
	const bootLogsActive = bootPreloaded && !refs.engineBootFinishedRef.current;
	const loadPanelActive =
		refs.sceneImportInProgressRef.current
		|| refs.sceneBurstLoadInProgressRef.current
		|| refs.engineBootAwaitRef.current
		|| bootLogsActive;
	if (loadPanelActive && SCENE_LOAD_SILENT_EVENTS.has(eventName)) return true;
	return false;
}

/** Línea legible para suelo/sol/jugador (arranque o escena FP vacía). */
function panelLogLineForBaselineSpawn(
	event: RuntimeEngineEvent,
	refs: Pick<
		EngineInternalRefs,
		| 'initialExtractDirRef'
		| 'fpSceneBaselineLogRef'
	>,
	projectType?: string,
): string | null {
	const openedFromSave = isProjectOpenedFromSave(refs.initialExtractDirRef.current);
	const bootPreloaded = isEngineBootScenePreloaded(projectType, openedFromSave);
	const fpBaseline = refs.fpSceneBaselineLogRef.current;
	const tag = bootPreloaded ? '[Carga]' : '[Escena]';

	if (event.event === 'model_loaded') {
		const name = (event as { name?: string }).name ?? '';
		if (name === 'Ground') {
			if (bootPreloaded || fpBaseline) return `${tag} Insertando suelo`;
			return null;
		}
		if (name.startsWith('Sun')) {
			if (bootPreloaded || fpBaseline) {
				return bootPreloaded
					? '[Carga] Insertando Sol (Sun)'
					: '[Escena] Insertando sol';
			}
			return null;
		}
	}

	if (event.event === 'character_loaded') {
		const path = (event as { path?: string }).path ?? '';
		if (!isPlayerPath(path)) return null;
		if (bootPreloaded || fpBaseline) {
			if (fpBaseline) endFpSceneBaselineLogging(refs.fpSceneBaselineLogRef);
			return `${tag} Insertando jugador placeholder`;
		}
	}

	return null;
}

type EditorSceneListItemPayload = { id: number; name: string; dirty?: boolean };

function formatEditorSceneList(scenes: EditorSceneListItemPayload[] | undefined): string {
	if (!scenes?.length) return '(vacía)';
	return scenes.map((s) => `«${s.name}»`).join(', ');
}

function panelLogLineForEditorSceneEvent(event: RuntimeEngineEvent): string | null | undefined {
	switch (event.event) {
		case 'editor_scenes_updated': {
			const activeId = event.active_scene_id as number | undefined;
			const scenes = event.scenes as EditorSceneListItemPayload[] | undefined;
			const reason = (event.update_reason as string | undefined) ?? 'sync';
			const activeName =
				scenes?.find((s) => s.id === activeId)?.name ?? (activeId != null ? `#${activeId}` : '?');
			const listSummary = `${scenes?.length ?? 0} escena/s: ${formatEditorSceneList(scenes)}`;
			switch (reason) {
				case 'boot':
					return `[Escenas] Registro inicializado — activa «${activeName}» (${listSummary})`;
				case 'project_saved':
					return `[Escenas] Registro alineado tras guardar — activa «${activeName}» (${listSummary})`;
				case 'project_loaded':
					return `[Escenas] Registro cargado — activa «${activeName}» (${listSummary})`;
				case 'scene_deleted':
					return `[Escenas] Escena eliminada — activa «${activeName}» (${listSummary})`;
				case 'undo_state':
				default:
					return null;
			}
		}
		case 'editor_scene_created': {
			const id = event.id as number | undefined;
			const name = (event.name as string | undefined) ?? '';
			const scenes = event.scenes as EditorSceneListItemPayload[] | undefined;
			return `[Escenas] Creada «${name}» (id=${id ?? '?'}); lista: ${formatEditorSceneList(scenes)}`;
		}
		case 'editor_scene_switched': {
			const activeId = event.active_scene_id as number | undefined;
			const scenes = event.scenes as EditorSceneListItemPayload[] | undefined;
			const activeName =
				scenes?.find((s) => s.id === activeId)?.name ?? (activeId != null ? `#${activeId}` : '?');
			return `[Escenas] Escena activa ${activeName} (id=${activeId ?? '?'})`;
		}
		case 'editor_scene_switch_blocked': {
			const reason = (event.reason as string | undefined) ?? 'unknown';
			const activeId = event.active_scene_id as number | undefined;
			const targetId = event.target_scene_id as number | undefined;
			const reasonLabel =
				reason === 'unsaved_changes'
					? 'guarda el proyecto antes de cambiar de escena'
					: reason;
			return `[Escenas] Cambio bloqueado (${activeId ?? '?'} → ${targetId ?? '?'}): ${reasonLabel}`;
		}
		default:
			return undefined;
	}
}

/** Línea `[trigger]` al entrar un actor en un execution area. */
function panelLogLineForTriggerEntered(
	event: RuntimeEngineEvent,
	entityMetaRef: EngineInternalRefs['entityMetaRef'],
): string {
	const triggerId = event.trigger_id as number;
	const actorId = event.actor_id as number;
	const hasFromEngine = event.has_attached_script as boolean | undefined;
	const meta = entityMetaRef.current[triggerId];
	const hasFromMeta = Boolean(
		meta?.scripts?.length
		|| meta?.visualScriptRhai?.trim()
		|| (meta?.visualGraph?.nodes?.length ?? 0) > 0,
	);
	const hasCode = hasFromEngine ?? hasFromMeta;
	const label = meta?.name?.trim() || `trigger ${triggerId}`;
	return hasCode
		? `[trigger] Activado «${label}» (id=${triggerId}, actor=${actorId}) — con código adjunto`
		: `[trigger] Activado «${label}» (id=${triggerId}, actor=${actorId}) — sin código adjunto`;
}

/** Línea del panel de logs; `null` = no imprimir; `undefined` = JSON del evento. */
function panelLogLineForEngineEvent(
	event: RuntimeEngineEvent,
	refs: Pick<
		EngineInternalRefs,
		| 'sceneImportInProgressRef'
		| 'sceneBurstLoadInProgressRef'
		| 'engineBootAwaitRef'
		| 'engineBootFinishedRef'
		| 'initialExtractDirRef'
		| 'sceneWorldCleanupRef'
		| 'fpSceneBaselineLogRef'
		| 'entityMetaRef'
	>,
	projectType?: string,
): string | null | undefined {
	if (event.event === 'trigger_entered') {
		return panelLogLineForTriggerEntered(event, refs.entityMetaRef);
	}

	const editorSceneLine = panelLogLineForEditorSceneEvent(event);
	if (editorSceneLine !== undefined) return editorSceneLine;

	if (event.event === 'entity_removed') {
		const cleanup = refs.sceneWorldCleanupRef.current;
		if (cleanup.active) {
			if (!cleanup.summaryLogged) {
				cleanup.summaryLogged = true;
				return '[Limpieza] Limpiando mundo para la nueva escena.';
			}
			return null;
		}
	}

	const baselineLine = panelLogLineForBaselineSpawn(event, refs, projectType);
	if (baselineLine != null) return baselineLine;

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

/** `blueprint_id` del IPC o blueprint activa en construcción rápida (colocación 3D). */
function resolvePlacementBlueprintId(
	loaded: {
		blueprint_id?: string
		rotation?: [number, number, number, number]
		physics_enabled?: boolean
	},
	refs: Pick<EngineInternalRefs, 'sceneBurstLoadInProgressRef' | 'quickBuildActiveBlueprintIdRef'>,
): string | undefined {
	if (loaded.blueprint_id) return loaded.blueprint_id;
	if (refs.sceneBurstLoadInProgressRef.current) return undefined;
	const fromQuickBuild =
		loaded.rotation != null && loaded.physics_enabled != null;
	if (!fromQuickBuild) return undefined;
	return refs.quickBuildActiveBlueprintIdRef.current ?? undefined;
}

function applyPlayCharacterDefaultsForPlayer(
	characterId: number,
	gameStyle: GameStyle | undefined,
	refs: EngineInternalRefs,
) {
	if (gameStyle !== 'first-person') return;
	// Proyecto abierto desde .save: bindings y entidades vienen del motor, no de la plantilla.
	if (isProjectOpenedFromSave(refs.initialExtractDirRef.current)) return;

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
			const openedFromSave = isProjectOpenedFromSave(refs.initialExtractDirRef.current);
			const engineLoads2dSave = is2dProjectLoadedByEngine(
				projectType,
				refs.initialExtractDirRef.current,
			);
			const engineLoads3dSave = is3dProjectLoadedByEngine(
				projectType,
				refs.initialExtractDirRef.current,
			);
			dispatch({ type: 'SET_READY' });
			dispatch({ type: 'SET_PREVIEW_PLAYING', payload: false });
			if (refs.readyTimer.current) clearTimeout(refs.readyTimer.current);
			const boot3dNoSave =
				isEngineBootScenePreloaded(projectType, openedFromSave) && !engineLoads3dSave;
			// Proyecto nuevo: plantilla vacía. Proyecto .save: el motor ya cargó desde extract_dir.
			if (projectType && !openedFromSave && !boot3dNoSave) {
				window.engine.send(
					buildSetSceneCommand(projectType, refs.initialSavePathRef.current) as never,
				);
			} else if (boot3dNoSave) {
				if (projectType === '3D' && gameStyle === 'first-person') {
					dispatch({ type: 'INIT_DEFAULT_FP_PLAYER_UI' });
				} else if (projectType === '2D') {
					dispatch({ type: 'INIT_DEFAULT_2D_PLAYER_UI' });
				}
				beginEngineBootEntityWait(refs);
			}
			window.engine.send({ cmd: 'set_preview_playing', playing: false } as never);
			refs.mainPlayerHandled.current = false;
			refs.playerRemoved.current = false;
			refs.pendingPlayerDups.current = [];
			refs.pendingDupQ.current = [];
			const motorGravity = typeof event.gravity === 'number' ? event.gravity : undefined;
			if (motorGravity != null) {
				dispatch({ type: 'SET_WORLD_CONFIG', payload: { gravity: motorGravity } });
			}
			if (engineLoads2dSave || engineLoads3dSave) {
				beginSceneImportLoading(dispatch, refs.sceneImportInProgressRef);
			} else if (boot3dNoSave) {
				queueMicrotask(() => {
					tryFinishEngineBootLoading(dispatch, refs, reportBounds);
				});
			} else {
				endEngineBootLoadingIfIdle(dispatch, refs, reportBounds);
			}
		}

		if (event.event === 'model_loaded') {
			trackEngineBootIpcSeen(refs, projectType, isProjectOpenedFromSave(refs.initialExtractDirRef.current));
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
				const burstCategory =
					normalizeBlueprintCategory(pending.entityCategory ?? loaded.entity_category)
					?? inferEntity3dCategoryFromName(pending.name ?? loaded.name);
				const isEnvironment = burstCategory === 'environment'
					|| pending.entityCategory === 'environment'
					|| loaded.entity_category === 'environment';
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
						? {
							entityCategory:
								(pending.entityCategory
									?? loaded.entity_category) as EntityCategory,
						}
						: {}),
					...(burstCategory
						? {
							entity3dCategory: reconcileCategoryWithName(
								burstCategory,
								pending.name ?? loaded.name,
							),
						}
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

			const placementBlueprintId = resolvePlacementBlueprintId(loaded, refs);
			if (placementBlueprintId && !refs.sceneBurstLoadInProgressRef.current) {
				let restorePending: PendingRestore | null = null;
				if (loaded.path) {
					const matched = takePendingRestoreByPath(
						refs.pendingRestoresRef.current,
						loaded.path,
					);
					if (matched) restorePending = matched.pending;
				}
				const bp = refs.blueprintsRef.current.find((b) => b.id === placementBlueprintId);
				const bpPhysics = bp ? blueprintPlacementPhysics(bp) : null;
				const placementCategory = bpPhysics?.placementCategory
					?? (bp ? blueprintPlacementCategory(bp) : undefined);
				const entityCategory =
					restorePending?.entityCategory
					?? (loaded.entity_category as EntityCategory | undefined)
					?? bpPhysics?.entityCategory
					?? (placementCategory
						? blueprintEntityCategoryForEngine(placementCategory)
						: undefined);
				const isEnvironment = entityCategory === 'environment'
					|| placementCategory === 'environment';
				const physicsEnabled = isEnvironment
					? true
					: (restorePending?.physicsEnabled
						?? loaded.physics_enabled
						?? bpPhysics?.physicsEnabled
						?? false);
				const physicsType = isEnvironment
					? 'static'
					: (restorePending?.physicsType
						?? loaded.physics_type
						?? bpPhysics?.physicsType
						?? 'static');
				const kind = (loaded.kind ?? 'model') as EntityMeta['kind'];
				const existing = refs.entityMetaRef.current[id];
				refs.entityMetaRef.current[id] = {
					...existing,
					kind,
					path: loaded.path ?? existing?.path ?? '',
					name: loaded.name ?? existing?.name ?? `Entity ${id}`,
					physicsEnabled,
					physicsType,
					...(placementCategory
						? {
							entity3dCategory: reconcileCategoryWithName(
								placementCategory,
								loaded.name,
							),
						}
						: {}),
					...(entityCategory ? { entityCategory } : {}),
					blueprintId: placementBlueprintId,
					...(restorePending?.scripts ? { scripts: restorePending.scripts } : {}),
					...(restorePending?.controlBindings
						? { controlBindings: restorePending.controlBindings }
						: {}),
				};
				if (loaded.position && loaded.scale) {
					refs.entityTransformsRef.current[id] = {
						position: loaded.position,
						rotation: loaded.rotation ?? restorePending?.transform?.rotation ?? [0, 0, 0, 1],
						scale: loaded.scale,
					};
				}
				const restoreFromBp = bp
					? buildQuickBuildPendingRestore(bp)
					: null;
				const effectiveRestore = restorePending ?? restoreFromBp;
				if (effectiveRestore) {
					sendApplyEntityRestore(id, effectiveRestore, {
						skipTransform: true,
						applyInitialAnimationFrame: false,
					});
					applyPendingRestoreMeta(refs, id, effectiveRestore);
				} else if (projectType === '3D' && physicsEnabled) {
					window.engine.send({
						cmd: 'set_physics',
						id,
						enabled: true,
						body_type: physicsType,
					} as never);
				}
				const loadPanelActive =
					refs.sceneImportInProgressRef.current
					|| refs.sceneBurstLoadInProgressRef.current;
				if (!loadPanelActive) {
					addLog(
						`[quick_build] entidad colocada: ${loaded.name ?? id} (id=${id})`,
					);
				}
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
			const ballQueue = refs.pendingRestoresRef.current.get('[Ball]');
			if (ballQueue && ballQueue.length > 0) {
				const pending = ballQueue.shift()!;
				refs.entityMetaRef.current[id] = {
					kind: 'model',
					path: '[Ball]',
					name: pending.name ?? loaded.name ?? `Entity ${id}`,
					physicsEnabled: pending.physicsEnabled ?? true,
					physicsType: pending.physicsType ?? 'dynamic',
					entityCategory: 'object',
					entity3dCategory: 'object',
				};
				sendApplyEntityRestore(id, pending, {
					skipTransform: true,
					applyInitialAnimationFrame: false,
				});
				applyPendingRestoreMeta(refs, id, pending);
				if (ballQueue.length === 0) refs.pendingRestoresRef.current.delete('[Ball]');
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
				const spawnModelPath = refs.pendingModelPathRef.current;
				const spawnKind = (loaded.kind ?? 'model') as EntityMeta['kind'];
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
				let modelPath = spawnModelPath ?? loadItem?.modelPath ?? loaded.path ?? null;
				let restorePending = loadItem?.pending;
				if (!restorePending && modelPath) {
					const qbQueue = refs.pendingRestoresRef.current.get(modelPath);
					if (qbQueue && qbQueue.length > 0) {
						restorePending = qbQueue.shift()!;
						if (qbQueue.length === 0) {
							refs.pendingRestoresRef.current.delete(modelPath);
						}
					}
				}
				if (!restorePending && loaded.path) {
					const matched = takePendingRestoreByPath(
						refs.pendingRestoresRef.current,
						loaded.path,
					);
					if (matched) {
						restorePending = matched.pending;
						modelPath = modelPath ?? matched.path;
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
				if (modelPath) {
					const placementBlueprintId =
						restorePending?.blueprintId ?? resolvePlacementBlueprintId(loaded, refs);
					const bp = placementBlueprintId
						? refs.blueprintsRef.current.find((b) => b.id === placementBlueprintId)
						: null;
					const bpPhysics = bp ? blueprintPlacementPhysics(bp) : null;
					const placementCategory = bp
						? blueprintPlacementCategory(bp)
						: spawnKind === 'character' || loaded.kind === 'character'
							? 'character'
							: normalizeBlueprintCategory(loaded.entity_category)
								?? (spawnCategory === 'environment'
									? 'environment'
									: spawnCategory === 'object'
										? 'object'
										: undefined)
								?? inferEntity3dCategoryFromName(loaded.name);
					const isEnvironment = spawnCategory === 'environment'
						|| restorePending?.entityCategory === 'environment'
						|| loaded.entity_category === 'environment'
						|| bpPhysics?.placementCategory === 'environment'
						|| placementCategory === 'environment';
					const physicsEnabled = isEnvironment
						? true
						: (restorePending?.physicsEnabled
							?? loaded.physics_enabled
							?? bpPhysics?.physicsEnabled
							?? false);
					const physicsType = isEnvironment
						? 'static'
						: (restorePending?.physicsType
							?? loaded.physics_type
							?? bpPhysics?.physicsType
							?? 'static');
					refs.entityMetaRef.current[id] = {
						kind: spawnKind,
						path: modelPath,
						name: restorePending?.name ?? loaded.name ?? `Entity ${id}`,
						physicsEnabled,
						physicsType,
						...(placementCategory
							? {
								entity3dCategory: reconcileCategoryWithName(
									placementCategory,
									restorePending?.name ?? loaded.name,
								),
							}
							: {}),
						...(isEnvironment ? { entityCategory: 'environment' as EntityCategory } : {}),
						...(restorePending?.entityCategory ? { entityCategory: restorePending.entityCategory } : {}),
						...(placementCategory === 'object'
							? { entityCategory: 'object' as EntityCategory }
							: {}),
						...(placementCategory === 'character'
							? { entity3dCategory: 'character' as const }
							: {}),
						scripts: restorePending?.scripts,
						controlBindings: restorePending?.controlBindings,
						...(placementBlueprintId ? { blueprintId: placementBlueprintId } : {}),
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
					if (projectType === '3D' && physicsEnabled) {
						window.engine.send({
							cmd: 'set_physics',
							id,
							enabled: true,
							body_type: physicsType,
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
				if (!refs.playerRemoved.current) {
					dispatch({ type: 'ADD_CHARACTER', payload: { id, path: '[Player]' } });
				}
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
				const engineLoads3dSave = is3dProjectLoadedByEngine(
					projectType,
					refs.initialExtractDirRef.current,
				);
				// El motor ya restaura jugador/cámara en `load_proyect`; IPC aquí usa refs obsoletos.
				const skipPlayViewIpcDuringEngineSaveLoad =
					engineLoads3dSave && refs.sceneImportInProgressRef.current;
				const savedView = savedPlayCharacterViewForRestore(
					refs.pendingPlayCharacterViewRef.current,
					refs.playCharacterViewRef.current,
				);
				if (replaced.position && !skipPlayViewIpcDuringEngineSaveLoad) {
					window.engine.send({
						cmd: 'set_play_character_view',
						position: replaced.position,
						...(savedView?.yaw !== undefined ? { yaw: savedView.yaw } : {}),
						...(savedView?.pitch !== undefined ? { pitch: savedView.pitch } : {}),
						...(savedView?.fov_y !== undefined ? { fov_y: savedView.fov_y } : {}),
						...(savedView?.frustum_distance !== undefined
							? { frustum_distance: savedView.frustum_distance }
							: {}),
						...(savedView?.camera_follow_mode
							? { camera_follow_mode: savedView.camera_follow_mode }
							: {}),
						...(savedView?.body_rotation ? { body_rotation: savedView.body_rotation } : {}),
						...(savedView?.body_scale ? { body_scale: savedView.body_scale } : {}),
						...(savedView?.camera_eye_position
							? { camera_eye_position: savedView.camera_eye_position }
							: {}),
						...(savedView?.fps_camera_yaw !== undefined
							? { fps_camera_yaw: savedView.fps_camera_yaw }
							: {}),
						...(savedView?.fps_camera_pitch !== undefined
							? { fps_camera_pitch: savedView.fps_camera_pitch }
							: {}),
					} as never);
				} else if (savedView?.position && !skipPlayViewIpcDuringEngineSaveLoad) {
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
					const bpScale = (bp as { scale?: [number, number, number] }).scale ?? [1, 1, 1];
					const bpRot = (bp as { rotation?: [number, number, number, number] }).rotation ?? [0, 0, 0, 1];
					const scaleChanged =
						Math.abs(selected.scale[0] - bpScale[0]) > 1e-4
						|| Math.abs(selected.scale[1] - bpScale[1]) > 1e-4
						|| Math.abs(selected.scale[2] - bpScale[2]) > 1e-4;
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
			if (activeEntityPropertiesHandlerRef.current) {
				pushEntityPropertiesPatch(activeEntityPropertiesHandlerRef.current);
			}
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
			window.engine.send({ cmd: 'get_fonts_list' } as never);
			window.engine.send({ cmd: 'get_hud_images_list' } as never);
			window.engine.send({ cmd: 'get_backgrounds_list' } as never);
			return;
		}

		if (event.event === 'player_ui_active_screen_changed') {
			const e = event as { screen_id?: string | null };
			const screenId =
				typeof e.screen_id === 'string' && e.screen_id.length > 0
					? e.screen_id
					: null;
			dispatch({ type: 'SET_ACTIVE_PLAYER_UI_SCREEN', payload: screenId });
		}

		if (event.event === 'project_loaded_3d') {
			const payload = event as unknown as ProjectLoaded3dPayload;
			refs.projectLoaded3dMetaRef.current = payload;
			refs.blueprintsRef.current = (payload.blueprints ?? []).map(blueprintFromSave);
			if (payload.player && payload.config_camera) {
				const view = playViewFromPlayerAndCamera(payload.player, payload.config_camera);
				refs.pendingPlayCharacterViewRef.current = view;
				refs.playCharacterViewRef.current = view;
			}
			if (payload.language === 'en' || payload.language === 'es') {
				setLocale?.(payload.language as Locale);
			}
			dispatch({
				type: 'APPLY_PROJECT_LOADED_3D',
				payload: {
					...payload,
					blueprints: refs.blueprintsRef.current,
				},
			});
			if (
				(payload.entityCount > 0 || payload.player)
				&& !refs.sceneImportInProgressRef.current
			) {
				beginSceneImportLoading(dispatch, refs.sceneImportInProgressRef);
			}
			window.engine.send({ cmd: 'get_models_list' } as never);
			window.engine.send({ cmd: 'get_sounds_list' } as never);
			window.engine.send({ cmd: 'get_fonts_list' } as never);
			window.engine.send({ cmd: 'get_hud_images_list' } as never);
			window.engine.send({ cmd: 'get_backgrounds_list' } as never);
			return;
		}

		if (event.event === 'project_load_2d_complete') {
			const engineLoads2dSave = is2dProjectLoadedByEngine(
				projectType,
				refs.initialExtractDirRef.current,
			);
			if (!engineLoads2dSave) return;
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
					for (const [idStr, meta] of Object.entries(refs.entityMetaRef.current)) {
						if (meta.kind !== 'character' || !meta.controlBindings) continue;
						window.engine.send({
							cmd: 'set_control_bindings',
							id: Number(idStr),
							bindings: meta.controlBindings,
						} as never);
					}
					if (scene.sprites?.length) {
						dispatch({ type: 'SET_LOADED_SPRITES_INFO', payload: scene.sprites });
					}
					dispatch({ type: 'SYNC_PLAY_CHARACTER_VIEW' });
				} catch (err) {
					console.error('[project_load_2d_complete] sync desde snapshot del motor:', err);
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

		if (event.event === 'project_load_3d_complete') {
			const engineLoads3dSave = is3dProjectLoadedByEngine(
				projectType,
				refs.initialExtractDirRef.current,
			);
			const motorFpSceneSwitch =
				projectType === '3D'
				&& gameStyle === 'first-person'
				&& refs.sceneImportInProgressRef.current;
			if (!engineLoads3dSave && !motorFpSceneSwitch) return;
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
					const playerId = refs.playerEntityIdRef.current;
					if (playerId != null && scene.player) {
						syncPlayerEntityMetaFromPlayer(refs, playerId, scene.player);
						if (scene.player.controls) {
							window.engine.send({
								cmd: 'set_control_bindings',
								id: playerId,
								bindings: scene.player.controls,
							} as never);
						}
						const playerView =
							scene.config_camera
								? playViewFromPlayerAndCamera(scene.player, scene.config_camera)
								: refs.playCharacterViewRef.current ??
									refs.pendingPlayCharacterViewRef.current;
						if (playerView) {
							refs.playCharacterViewRef.current = playerView;
							refs.pendingPlayCharacterViewRef.current = playerView;
							applySavedPlayCharacterView(playerView);
						}
						applyPlayCharacterControlDefaultsIfEmpty(playerId, refs.entityMetaRef, (cmd) => {
							window.engine.send(cmd as never);
						});
						refs.mainPlayerHandled.current = true;
					}
					if (scene.models?.length) {
						for (const model of scene.models) {
							dispatch({
								type: 'SYNC_MODEL_PRELOAD',
								payload: {
									path: model.path,
									name: model.name,
									...(model.category ? { category: model.category } : {}),
								},
							});
						}
					}
					window.engine.send({ cmd: 'get_models_list' } as never);
					window.engine.send({ cmd: 'resend_all_model_clips' } as never);
					dispatch({ type: 'SYNC_PLAY_CHARACTER_VIEW' });
					if (activeEntityPropertiesHandlerRef.current) {
						pushEntityPropertiesPatch(activeEntityPropertiesHandlerRef.current);
					}
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
			const characterPath = (event as { path?: string }).path ?? '';
			if (refs.sceneImportInProgressRef.current && !isPlayerPath(characterPath)) return;
			trackEngineBootIpcSeen(refs, projectType, isProjectOpenedFromSave(refs.initialExtractDirRef.current));
			const character = event as unknown as CharacterLoaded;
			const applyPendingRestore = (
				id: number,
				path: string,
				options?: { skipTransform?: boolean; omitScale?: boolean },
			) => {
				const queue = refs.pendingRestoresRef.current.get(path);
				if (!queue || queue.length === 0) return false;

				const pending = queue.shift()!;
				const isPlayer = isPlayerPath(path);
				sendApplyEntityRestore(id, pending, {
					omitScale: options?.omitScale ?? isPlayer,
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
							physicsEnabled: false,
							physicsType: '',
						};
					} else {
						refs.entityMetaRef.current[character.id].physicsEnabled = false;
						refs.entityMetaRef.current[character.id].physicsType = '';
					}
					const savedFpView = savedPlayCharacterViewForRestore(
						refs.pendingPlayCharacterViewRef.current,
						refs.playCharacterViewRef.current,
					);
					const awaitingVisualReplace = applyPendingRestore(character.id, character.path, {
						skipTransform: Boolean(savedFpView?.position),
						omitScale: !savedFpView?.body_scale,
					});
					if (!awaitingVisualReplace && !refs.sceneImportInProgressRef.current) {
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

		if (event.event === 'model_asset_load_failed') {
			const model = event as unknown as { path: string; name?: string; model_id?: string };
			const entry = refs.modelsRef.current.find(
				(m) =>
					m.loading
					&& (m.path === model.path
						|| pathsMatchForBurstRestore(m.path, model.path)),
			);
			dispatch({
				type: 'MARK_MODEL_READY',
				payload: {
					path: model.path,
					name: entry?.name ?? model.path.split(/[/\\]/).pop() ?? 'model',
					...(model.model_id ? { model_id: model.model_id } : {}),
					state: 'failed',
				},
			});
		}

		if (event.event === 'model_asset_importing') {
			const model = event as unknown as { path: string; name: string; model_id: string };
			dispatch({
				type: 'ADD_MODEL_INFO',
				payload: {
					path: model.path,
					name: model.name,
					loading: true,
					model_id: model.model_id,
					state: 'importing',
				},
			});
		}

		if (event.event === 'model_asset_imported') {
			const model = event as unknown as {
				path: string
				name: string
				model_id: string
				asset: string
			};
			dispatch({
				type: 'MARK_MODEL_READY',
				payload: {
					path: model.path,
					name: model.name,
					model_id: model.model_id,
					asset: model.asset,
					state: 'importing',
				},
			});
		}

		if (event.event === 'model_asset_loaded') {
			const model = event as unknown as {
				path: string
				name: string
				model_id?: string
			};
			dispatch({
				type: 'MARK_MODEL_READY',
				payload: {
					path: model.path,
					name: model.name,
					...(model.model_id ? { model_id: model.model_id } : {}),
					state: 'ready',
				},
			});
			window.engine.send({ cmd: 'get_models_list' } as never);
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
			const modelsList = event as unknown as {
				models: import('@shared-types').ModelInfo[];
			};
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

		if (event.event === 'font_loaded') {
			const font = event as unknown as { path: string; name: string };
			dispatch({ type: 'ADD_FONT', payload: { path: font.path, name: font.name } });
		}

		if (event.event === 'font_removed') {
			const font = event as unknown as { path: string };
			dispatch({ type: 'REMOVE_FONT', payload: font.path });
		}

		if (event.event === 'fonts_list') {
			const fontsList = event as unknown as { fonts: { path: string; name: string }[] };
			dispatch({ type: 'SET_FONTS', payload: fontsList.fonts });
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

		if (event.event === 'player_ui_text_box_added') {
			const e = event as {
				id?: number;
				font_name?: string;
				text?: string;
				z_index?: number;
				locked?: boolean;
			};
			if (typeof e.id === 'number') {
				dispatch({
					type: 'ADD_PLAYER_UI_TEXT_BOX',
					payload: {
						id: e.id,
						fontName: e.font_name ?? '',
						text: e.text ?? '',
						zIndex: typeof e.z_index === 'number' ? e.z_index : 0,
						locked: Boolean(e.locked),
					},
				});
			}
		}

		if (event.event === 'player_ui_text_box_updated') {
			const e = event as { id?: number; text?: string };
			if (typeof e.id === 'number' && typeof e.text === 'string') {
				dispatch({
					type: 'UPDATE_PLAYER_UI_TEXT_BOX',
					payload: { id: e.id, text: e.text },
				});
			}
		}

		if (event.event === 'player_ui_text_box_removed') {
			const e = event as { id?: number };
			if (typeof e.id === 'number') {
				dispatch({ type: 'REMOVE_PLAYER_UI_TEXT_BOX', payload: e.id });
			}
		}

		if (event.event === 'player_ui_text_boxes_list') {
			const e = event as {
				boxes?: Array<{
					id?: number;
					font_name?: string;
					text?: string;
					z_index?: number;
					locked?: boolean;
				}>;
				buttons?: Array<{
					id?: number;
					font_name?: string;
					text?: string;
					z_index?: number;
					locked?: boolean;
				}>;
				images?: Array<{
					id?: number;
					image_name?: string;
					z_index?: number;
					locked?: boolean;
				}>;
				objects?: Array<{
					id?: number;
					vertex_count?: number;
					z_index?: number;
					locked?: boolean;
				}>;
			};
			const textBoxes = (e.boxes ?? [])
				.filter((b): b is { id: number; font_name?: string; text?: string } =>
					typeof b.id === 'number',
				)
				.map((b) => ({
					id: b.id,
					fontName: b.font_name ?? '',
					text: b.text ?? '',
					zIndex: typeof b.z_index === 'number' ? b.z_index : 0,
					locked: Boolean(b.locked),
				}));
			const buttons = (e.buttons ?? [])
				.filter((b): b is { id: number; font_name?: string; text?: string } =>
					typeof b.id === 'number',
				)
				.map((b) => ({
					id: b.id,
					fontName: b.font_name ?? '',
					text: b.text ?? '',
					zIndex: typeof b.z_index === 'number' ? b.z_index : 0,
					locked: Boolean(b.locked),
				}));
			const images = (e.images ?? [])
				.filter((img): img is { id: number; image_name?: string } =>
					typeof img.id === 'number',
				)
				.map((img) => ({
					id: img.id,
					imageName: img.image_name ?? '',
					zIndex: typeof img.z_index === 'number' ? img.z_index : 0,
					locked: Boolean(img.locked),
				}));
			const objects = (e.objects ?? [])
				.filter((obj): obj is { id: number; vertex_count?: number } =>
					typeof obj.id === 'number',
				)
				.map((obj) => ({
					id: obj.id,
					vertexCount:
						typeof obj.vertex_count === 'number' ? obj.vertex_count : 0,
					fillColor: Array.isArray(obj.fill_color) && obj.fill_color.length === 4
						? obj.fill_color as [number, number, number, number]
						: undefined,
					texturePath: typeof obj.texture_path === 'string' ? obj.texture_path : null,
					textureName: typeof obj.texture_name === 'string' ? obj.texture_name : '',
					zIndex: typeof obj.z_index === 'number' ? obj.z_index : 0,
					locked: Boolean(obj.locked),
				}));
			dispatch({
				type: 'SET_EDITING_UI_ELEMENTS',
				payload: buildEditingUiElementsFromEngineList({
					textBoxes,
					buttons,
					images,
					objects,
					buttonDefaultConfig: DEFAULT_PLAYER_UI_BUTTON_CONFIG,
				}),
			});
		}

		if (event.event === 'player_ui_button_added') {
			const e = event as { id?: number; text?: string; font_name?: string };
			if (typeof e.id === 'number') {
				const base =
					takePendingPlayerUiButtonConfig() ?? DEFAULT_PLAYER_UI_BUTTON_CONFIG;
				dispatch({
					type: 'ADD_PLAYER_UI_BUTTON',
					payload: {
						id: e.id,
						config: {
							...base,
							text: e.text ?? base.text,
							fontName: e.font_name ?? base.fontName,
						},
					},
				});
			}
		}

		if (event.event === 'player_ui_button_removed') {
			const e = event as { id?: number };
			if (typeof e.id === 'number') {
				dispatch({ type: 'REMOVE_PLAYER_UI_BUTTON', payload: e.id });
			}
		}

		if (event.event === 'player_ui_image_added') {
			const e = event as {
				id?: number;
				image_name?: string;
				z_index?: number;
				locked?: boolean;
			};
			if (typeof e.id === 'number') {
				dispatch({
					type: 'ADD_PLAYER_UI_IMAGE',
					payload: {
						id: e.id,
						imageName: e.image_name ?? '',
						zIndex: typeof e.z_index === 'number' ? e.z_index : 0,
						locked: Boolean(e.locked),
					},
				});
			}
		}

		if (event.event === 'player_ui_object_added') {
			// Sidebar vía `player_ui_text_boxes_list`; aquí solo cerramos el modo dibujo.
			dispatch({ type: 'PLAYER_UI_OBJECT_DRAW_END' });
		}

		if (event.event === 'player_ui_object_draw_ended') {
			dispatch({ type: 'PLAYER_UI_OBJECT_DRAW_END' });
		}

		if (event.event === 'player_ui_object_removed') {
			const e = event as { id?: number };
			if (typeof e.id === 'number') {
				dispatch({ type: 'REMOVE_PLAYER_UI_OBJECT', payload: e.id });
			}
		}

		if (event.event === 'player_ui_image_removed') {
			const e = event as { id?: number };
			if (typeof e.id === 'number') {
				dispatch({ type: 'REMOVE_PLAYER_UI_IMAGE', payload: e.id });
			}
		}

		if (event.event === 'hud_image_loaded') {
			const e = event as { path?: string; name?: string };
			if (e.path && e.name) {
				dispatch({ type: 'ADD_HUD_IMAGE', payload: { path: e.path, name: e.name } });
			}
		}

		if (event.event === 'hud_image_removed') {
			const e = event as { path?: string };
			if (e.path) {
				dispatch({ type: 'REMOVE_HUD_IMAGE', payload: e.path });
			}
		}

		if (event.event === 'hud_images_list') {
			const list = event as { images?: Array<{ path?: string; name?: string }> };
			const images = (list.images ?? [])
				.filter((img): img is { path: string; name: string } =>
					Boolean(img.path && img.name),
				)
				.map((img) => ({ path: img.path, name: img.name }));
			dispatch({ type: 'SET_HUD_IMAGES', payload: images });
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
			const collider = event as {
				id?: number
				points?: [[number, number], [number, number], [number, number], [number, number]]
				position?: [number, number, number]
				scale?: [number, number, number]
			};
			const id = collider.id ?? -1;
			const transformFrom3d =
				collider.position && collider.scale
					? {
							position: collider.position,
							rotation: [0, 0, 0, 1] as [number, number, number, number],
							scale: collider.scale,
						}
					: null;
			refs.entityMetaRef.current[id] = {
				kind: 'collider',
				path: '[Colisionador]',
				physicsEnabled: true,
				physicsType: 'static',
				points: collider.points,
			};
			const transformFromPoints = buildTransformFromPoints(collider.points);
			if (transformFrom3d) {
				refs.entityTransformsRef.current[id] = transformFrom3d;
			} else if (transformFromPoints) {
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
			const area = event as {
				id?: number
				points?: [[number, number], [number, number], [number, number], [number, number]]
				position?: [number, number, number]
				scale?: [number, number, number]
			};
			const id = area.id ?? -1;
			const transformFrom3d =
				area.position && area.scale
					? {
							position: area.position,
							rotation: [0, 0, 0, 1] as [number, number, number, number],
							scale: area.scale,
						}
					: null;
			refs.entityMetaRef.current[id] = {
				kind: 'execution_area',
				path: '[ExecutionArea]',
				physicsEnabled: false,
				physicsType: 'static',
				points: area.points,
			};
			const transformFromPoints = buildTransformFromPoints(area.points);
			if (transformFrom3d) {
				refs.entityTransformsRef.current[id] = transformFrom3d;
			} else if (transformFromPoints) {
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
			if (activeEntityPropertiesHandlerRef.current) {
				pushEntityPropertiesPatch(activeEntityPropertiesHandlerRef.current);
			}
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
			if (activeEntityPropertiesHandlerRef.current) {
				pushEntityPropertiesPatch(activeEntityPropertiesHandlerRef.current);
			}
		}

		if (event.event === 'graphics_texture_tier_changed') {
			const tierEvent = event as unknown as { tier: string }
			const tier = tierEvent.tier
			if (tier === 'low' || tier === 'medium' || tier === 'high' || tier === 'ultra') {
				dispatch({ type: 'SET_GRAPHICS_TEXTURE_TIER', payload: tier })
				if (activeEntityPropertiesHandlerRef.current) {
					pushEntityPropertiesPatch(activeEntityPropertiesHandlerRef.current)
				}
			}
		}

		if (event.event === 'entity_textures_ready') {
			const texEvent = event as unknown as {
				entity_id: number
				model_path: string
				active_tier?: string
				materials: Array<{
					materialIndex: number
					materialName: string
					defaultImageIndex?: number
					variants: Array<{ imageIndex: number; width: number; height: number }>
					tierImageIndex: Record<string, number>
					previewTier: string
				}>
			}
			const id = texEvent.entity_id
			if (
				texEvent.active_tier === 'low'
				|| texEvent.active_tier === 'medium'
				|| texEvent.active_tier === 'high'
				|| texEvent.active_tier === 'ultra'
			) {
				dispatch({ type: 'SET_GRAPHICS_TEXTURE_TIER', payload: texEvent.active_tier })
			}
			const materials = (texEvent.materials ?? []).map((m) => ({
				materialIndex: m.materialIndex,
				materialName: m.materialName,
				defaultImageIndex: m.defaultImageIndex,
				variants: (m.variants ?? []).map((v) => ({
					imageIndex: v.imageIndex,
					width: v.width,
					height: v.height,
				})),
				tierImageIndex: (m.tierImageIndex ?? {}) as import('../../../modal-electron/entityPropertiesTypes').EntityMaterialTextures['tierImageIndex'],
				previewTier: (m.previewTier || 'low') as import('../../../modal-electron/entityPropertiesTypes').GraphicsTextureTier,
			}))
			const prevMeta = refs.entityMetaRef.current[id]
			refs.entityMetaRef.current[id] = {
				kind: 'model',
				path: texEvent.model_path,
				...prevMeta,
				entityTextures: materials,
				entityTexturesModelPath: texEvent.model_path,
				visualModelPath: prevMeta?.visualModelPath ?? texEvent.model_path,
			}
			if (activeEntityPropertiesHandlerRef.current) {
				pushEntityPropertiesPatch(activeEntityPropertiesHandlerRef.current)
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
			if (activeEntityPropertiesHandlerRef.current) {
				pushEntityPropertiesPatch(activeEntityPropertiesHandlerRef.current);
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
			if (activeEntityPropertiesHandlerRef.current) {
				pushEntityPropertiesPatch(activeEntityPropertiesHandlerRef.current);
			}
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