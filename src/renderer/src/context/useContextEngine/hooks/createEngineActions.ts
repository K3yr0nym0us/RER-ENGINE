import type { Dispatch } from 'react';
import type { ModelCategory } from '@shared-types';
import type {
	BluePrintEntry,
	EngineAction,
	EngineInternalRefs,
	EntityAnimations,
	EntityMeta,
	EntityScripts,
	Transform,
	UiScreenEntry,
	UiScreenScope,
	WorldConfig,
	PlayerUiButtonConfig,
	GraphicsTextureTier,
	ReflectionTier,
	ReflectionDebugView,
	ShadowTier,
	MsaaTier,
} from '../types';
import { normalizeGraphicsTextureTier, normalizeReflectionTier, normalizeReflectionDebugView, normalizeShadowTier, normalizeMsaaTier } from '../types';
import { buildPlayAnimationFrameCmd } from './applyPendingRestoreToEngine';
import { beginModelReplaceLoading, endModelReplaceLoading } from './sceneImportOverlay';
import { invalidateEntityBoneNames } from '../../../utils/entity3dEditorSync';
import {
	blueprintEntityCategoryForEngine,
	blueprintPlacementCategory,
	blueprintUsesModel3D,
	buildBlueprintPlacementMeta,
	normalizeBlueprintCategory,
	reconcileCategoryWithName,
} from '../../../utils/blueprintModelPath';
import { applyPlayCharacterControlDefaultsIfEmpty } from '../../../defaults/applyPlayCharacterControlDefaults';
import type { EngineCommand2D, EngineCommand3D } from '@shared-types';

let uiScreenIdCounter = 0;
let pendingPlayerUiButtonConfig: PlayerUiButtonConfig | null = null;

/** Config del modal de botón pendiente de correlacionar con `player_ui_button_added`. */
export function takePendingPlayerUiButtonConfig(): PlayerUiButtonConfig | null {
	const config = pendingPlayerUiButtonConfig;
	pendingPlayerUiButtonConfig = null;
	return config;
}

const createUiScreenId = () => {
	uiScreenIdCounter += 1;
	return `ui_${uiScreenIdCounter}`;
};

interface CreateEngineActionsParams {
	dispatch: Dispatch<EngineAction>
	refs: EngineInternalRefs
	addLog: (text: string, isError?: boolean) => void
	reportBounds: () => void
	send: (cmd: EngineCommand2D | EngineCommand3D) => void
	send2d: (cmd: EngineCommand2D) => void
	send3d: (cmd: EngineCommand3D) => void
	projectType?: string
}

export function createEngineActions({
	dispatch,
	refs,
	addLog,
	reportBounds,
	send,
	send2d: send2dFn,
	send3d: send3dFn,
	projectType,
}: CreateEngineActionsParams) {
	const is3D = projectType === '3D';
	const sendMotor = is3D ? send3dFn : send2dFn;
	const supportsPlayerUi = projectType === '2D' || projectType === '3D';

	const sendAsync = <T,>(cmd: EngineCommand2D | EngineCommand3D, waitForEvent: string, onStart?: () => void): Promise<T> => {
		if (onStart) onStart();
		return new Promise((resolve) => {
			refs.pendingEventsRef.current.set(waitForEvent, {
				resolve: (value: unknown) => {
					resolve(value as T);
				},
			});
			send(cmd);
		});
	};

	const setAnimationPlaying = (
		entityId: number,
		playing: boolean,
		animationName?: string | null,
	) => {
		dispatch({ type: 'SET_ANIMATION_PLAYING', payload: { entityId, playing } });
		const meta = refs.entityMetaRef.current[entityId];
		if (!meta) return;
		if (playing && animationName) {
			meta.playingAnimationName = animationName;
		} else if (!playing) {
			delete meta.playingAnimationName;
		}
	};

	const applyInitialAnimationFrame = (entityId: number, animations?: EntityAnimations) => {
		if (!animations || animations.length === 0) return;

		const defaultAnim = animations.find((anim) => anim?.is_default);
		if (!defaultAnim) return;
		const firstFrame = defaultAnim.frames?.[0];
		if (!firstFrame?.path) return;

		send2dFn(
			buildPlayAnimationFrameCmd(entityId, defaultAnim, firstFrame),
		);

		dispatch({ type: 'SET_ANIMATION_PLAYING', payload: { entityId, playing: false } });
	};

	const loadModelAsset = (
		path: string,
		name: string,
		category?: ModelCategory,
	) => {
		// Si quedó overlay de replace por una operación previa, no debe bloquear
		// la carga de recursos en Models (debe seguir interactivo).
		if (
			refs.modelReplaceInProgressRef.current
			&& !refs.sceneImportInProgressRef.current
			&& !refs.sceneBurstLoadInProgressRef.current
		) {
			endModelReplaceLoading(
				dispatch,
				refs.modelReplaceInProgressRef,
				refs.sceneImportInProgressRef,
				refs.sceneBurstLoadInProgressRef,
				reportBounds,
				refs.modelLoadOverlayKindRef,
				refs,
			);
		}
		send3dFn({
			cmd: 'load_model_asset',
			path,
			name,
			...(category ? { category } : {}),
		});
		dispatch({ type: 'ADD_MODEL_INFO', payload: { path, name, loading: true, category } });
	};

	const isModelPreloadReady = (path: string): boolean => {
		const entry = refs.modelsRef.current.find((m) => m.path === path);
		return entry != null && entry.loading !== true;
	};

	const removeModelAsset = (path: string) => {
		send3dFn({ cmd: 'remove_model_asset', path });
		dispatch({ type: 'REMOVE_MODEL_INFO', payload: path });
	};

	const getModelsList = () => {
		send3dFn({ cmd: 'get_models_list' });
	};

	const spawnModel = (path: string, kind: EntityMeta['kind'] = 'model', category?: EntityMeta['entityCategory']) => {
		refs.pendingModelPathRef.current = path;
		refs.pendingSpawnCategoryRef.current = category ?? null;
		if (!isModelPreloadReady(path)) {
			beginModelReplaceLoading(
				dispatch,
				refs.modelReplaceInProgressRef,
				'entity',
				refs.modelLoadOverlayKindRef,
			);
		}
		const entityCategory =
			category === 'environment'
				? 'environment'
				: category === 'object'
					? 'object'
					: category === 'weapon'
						? 'weapon'
						: category === 'projectile'
							? 'projectile'
							: kind === 'character'
								? 'character'
								: undefined;
		sendMotor({
			cmd: 'load_model',
			path,
			single_instance: true,
			kind,
			...(entityCategory ? { entity_category: entityCategory } : {}),
		});
	};

	const replaceEntityModel = (entityId: number, modelPath: string) => {
		const meta = refs.entityMetaRef.current[entityId];
		if (meta) {
			invalidateEntityBoneNames(meta);
			meta.visualModelPath = modelPath;
			if (/\.(glb|gltf|fbx)$/i.test(modelPath)) {
				meta.path = modelPath;
			}
			meta.animations = [];
		}
		dispatch({
			type: 'UPDATE_ENTITY_ANIMATIONS',
			payload: { entityId, animations: [], visualModelPath: modelPath },
		});
		dispatch({
			type: 'SET_ANIMATION_PLAYING',
			payload: { entityId, playing: false },
		});
		// Mostrar overlay siempre durante replace para evitar parpadeo gris del viewport
		// cuando la operación bloquea el hilo del motor, incluso con modelo ya precargado.
		beginModelReplaceLoading(
			dispatch,
			refs.modelReplaceInProgressRef,
			'model',
			refs.modelLoadOverlayKindRef,
		);
		sendMotor({ cmd: 'replace_entity_model', id: entityId, path: modelPath });
	};

	const retryEngine = () => {
		dispatch({ type: 'RESET_ENGINE' });
		addLog('[retry] Reiniciando motor…');
		reportBounds();
	};

	const removeEntity = (id: number) => {
		sendMotor({ cmd: 'remove_entity', id });
		dispatch({ type: 'REMOVE_ENTITY', payload: id });
		if (refs.playerEntityIdRef.current === id) refs.playerEntityIdRef.current = null;
		delete refs.entityMetaRef.current[id];
		delete refs.entityTransformsRef.current[id];
	};

	const removeScenario = (id: number) => removeEntity(id);
	const removeCharacter = (id: number) => removeEntity(id);

	const setWorldSize = (width: number, height: number, depth?: number) => {
		const payload = typeof depth === 'number'
			? { worldWidth: width, worldHeight: height, worldDepth: depth }
			: { worldWidth: width, worldHeight: height };
		dispatch({ type: 'SET_WORLD_CONFIG', payload });
		sendMotor({ cmd: 'set_world_size', width, height, depth });
	};

	const setWorldRadius = (radius: number) => {
		const normalized = Math.max(5, Math.min(500, radius));
		dispatch({
			type: 'SET_WORLD_CONFIG',
			payload: {
				worldRadius: normalized,
				worldWidth: normalized * 2,
				worldHeight: normalized * 2,
				worldDepth: normalized * 2,
			},
		});
		send3dFn({ cmd: 'set_world_radius', radius: normalized } as never);
	};

	const setGridVisible = (visible: boolean) => {
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { gridVisible: visible } });
		sendMotor({ cmd: 'set_grid_visible', visible });
	};

	const setGridCellSize = (size: number) => {
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { gridCellSize: size } });
		sendMotor({ cmd: 'set_grid_cell_size', size });
	};

	const setGravity = (gravity: number) => {
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { gravity } });
		sendMotor({ cmd: 'set_gravity', gravity });
	};

	const setDirectionalLight = (settings: {
		ambient?: number;
		intensity?: number;
		shadowDarkness?: number;
	}) => {
		const payload: Partial<WorldConfig> = {};
		if (settings.ambient !== undefined) payload.lightAmbient = settings.ambient;
		if (settings.intensity !== undefined) payload.lightIntensity = settings.intensity;
		if (settings.shadowDarkness !== undefined) payload.shadowDarkness = settings.shadowDarkness;
		if (Object.keys(payload).length > 0) {
			dispatch({ type: 'SET_WORLD_CONFIG', payload });
		}
		send3dFn({
			cmd: 'set_directional_light',
			ambient: settings.ambient,
			intensity: settings.intensity,
			shadow_darkness: settings.shadowDarkness,
		} as never);
	};

	const setTargetFps = (fps: number) => {
		const parsedFps = Number.isFinite(fps) ? fps : 60;
		const normalizedFps = Math.max(1, Math.min(1000, Math.round(parsedFps)));
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { targetFps: normalizedFps } });
		sendMotor({ cmd: 'set_target_fps', fps: normalizedFps });
	};

	const setGraphicsTextureTier = (tier: GraphicsTextureTier) => {
		const normalized = normalizeGraphicsTextureTier(tier);
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { graphicsTextureTier: normalized } });
		send3dFn({ cmd: 'set_graphics_texture_tier', tier: normalized });
	};

	const setReflectionTier = (tier: ReflectionTier) => {
		const normalized = normalizeReflectionTier(tier);
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { reflectionTier: normalized } });
		send3dFn({ cmd: 'set_reflection_tier', tier: normalized });
		addLog(`[Reflejos] Nivel solicitado: ${normalized}`);
	};

	const setReflectionRaytracing = (enabled: boolean) => {
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { reflectionRaytracing: enabled } });
		send3dFn({ cmd: 'set_reflection_raytracing', enabled });
		addLog(`[Reflejos] Ray tracing: ${enabled ? 'on' : 'off'}`);
	};

	const setReflectionProbes = (enabled: boolean) => {
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { reflectionProbes: enabled } });
		send3dFn({ cmd: 'set_reflection_probes', enabled });
		addLog(`[Reflejos] Probes: ${enabled ? 'on' : 'off'}`);
	};

	const spawnReflectionProbe = () => {
		send3dFn({ cmd: 'spawn_reflection_probe' });
		addLog('[Reflejos] Insertando sonda de reflejo…');
	};

	const setShadowTier = (tier: ShadowTier) => {
		const normalized = normalizeShadowTier(tier);
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { shadowTier: normalized } });
		send3dFn({ cmd: 'set_shadow_tier', tier: normalized });
		addLog(`[Sombras] Nivel solicitado: ${normalized}`);
	};

	const setMsaaTier = (tier: MsaaTier) => {
		const normalized = normalizeMsaaTier(tier);
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { msaaTier: normalized } });
		send3dFn({ cmd: 'set_msaa_tier', tier: normalized });
		addLog(`[MSAA] Nivel solicitado: ${normalized}`);
	};

	const setTaaEnabled = (enabled: boolean) => {
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { taaEnabled: enabled } });
		send3dFn({ cmd: 'set_taa', enabled });
		addLog(`[TAA] ${enabled ? 'Activado' : 'Desactivado'}`);
	};

	const setTaaParams = (params: { blend: number; jitterScale: number; enabled: boolean }) => {
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { taaBlend: params.blend, taaJitterScale: params.jitterScale } });
		send3dFn({ cmd: 'set_taa', enabled: params.enabled, blend: params.blend, jitter_scale: params.jitterScale });
		// Params log suppressed
	};

	const setReflectionDebugView = (view: ReflectionDebugView | string) => {
		const normalized = normalizeReflectionDebugView(view);
		const isSsrDebug = normalized === 'ssr_debug';
		dispatch({
			type: 'SET_WORLD_CONFIG',
			payload: { reflectionDebugView: normalized, ssrDebugMode: isSsrDebug },
		});
		send3dFn({ cmd: 'set_reflection_debug_view', view: normalized });
		addLog(
			isSsrDebug
				? '[SSR debug] activado: aciertos en pantalla + logs en consola del motor'
				: `[Reflejos] Vista debug: ${normalized}`,
		);
	};

	const setSsrDebugMode = (enabled: boolean) => {
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { ssrDebugMode: enabled } });
		send3dFn({ cmd: 'set_ssr_debug_mode', enabled });
		addLog(`[SSR debug] modo depuración: ${enabled ? 'activado' : 'desactivado'}`);
	};

	const setTextureDetailDistance = (distanceM: number) => {
		const normalized = Math.max(1, Math.min(500, distanceM));
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { textureDetailDistance: normalized } });
		send3dFn({ cmd: 'set_texture_detail_distance', distance_m: normalized });
	};

	const removeCollider = (id: number) => removeEntity(id);
	const removeExecutionArea = (id: number) => removeEntity(id);

	const updateEntityAnimations = (id: number, animations: EntityAnimations): EntityAnimations => {
		const bpId = refs.entityMetaRef.current[id]?.blueprintId;

		const applyAnimationsToSingleEntity = (entityId: number) => {
			if (!refs.entityMetaRef.current[entityId]) return;
			refs.entityMetaRef.current[entityId].animations = animations;
			for (const anim of animations) {
				sendMotor({
					cmd: 'set_animation',
					id: entityId,
					name: anim.name,
					frames: anim.frames,
					fps: anim.fps,
					loop_: anim.loop,
					flip_horizontal: !(anim.facing_right ?? true),
					audio_path: anim.audio_path ?? null,
					scripts: anim.scripts ?? [],
					is_cancelable: anim.is_cancelable ?? true,
					logical_w: anim.logical_w > 0 ? anim.logical_w : undefined,
					logical_h: anim.logical_h > 0 ? anim.logical_h : undefined,
				});
			}
			const defaultAnim = animations.find((anim) => anim?.is_default);
			if (defaultAnim?.name) {
				sendMotor({ cmd: 'set_default_animation', id: entityId, name: defaultAnim.name });
			} else {
				sendMotor({ cmd: 'set_default_animation', id: entityId, name: '' });
			}
		};

		if (bpId) {
			const updatedBlueprints = refs.blueprintsRef.current.map((bp) =>
				bp.id === bpId ? { ...bp, animations } : bp
			);
			refs.blueprintsRef.current = updatedBlueprints;
			dispatch({ type: 'SET_BLUEPRINTS', payload: updatedBlueprints });
			for (const [entityIdStr, meta] of Object.entries(refs.entityMetaRef.current)) {
				if (meta.blueprintId === bpId) {
					applyAnimationsToSingleEntity(Number(entityIdStr));
				}
			}
		} else {
			if (!refs.entityMetaRef.current[id]) {
				refs.entityMetaRef.current[id] = { kind: 'model', path: '', physicsEnabled: false, physicsType: '' };
			}
			applyAnimationsToSingleEntity(id);
		}

		const resolved = refs.entityMetaRef.current[id]?.animations ?? animations;
		if (bpId) {
			refs.blueprintsRef.current = refs.blueprintsRef.current.map((bp) =>
				bp.id === bpId ? { ...bp, animations: resolved } : bp
			);
			dispatch({ type: 'SET_BLUEPRINTS', payload: refs.blueprintsRef.current });
		}
		dispatch({ type: 'UPDATE_ENTITY_ANIMATIONS', payload: { entityId: id, animations: resolved } });
		return resolved;
	};

	const updateEntityScripts = (id: number, scripts: EntityScripts) => {
		const bpId = refs.entityMetaRef.current[id]?.blueprintId;

		const applyScriptsToSingleEntity = (entityId: number) => {
			if (!refs.entityMetaRef.current[entityId]) return;
			refs.entityMetaRef.current[entityId].scripts = scripts;
			for (const script of scripts) {
				sendMotor({ cmd: 'load_script', id: entityId, path: script.name, source: script.source });
			}
		};

		if (bpId) {
			// Actualizar la blueprint y propagar a todas sus instancias
			const updatedBlueprints = refs.blueprintsRef.current.map((bp) =>
				bp.id === bpId ? { ...bp, scripts } : bp
			);
			refs.blueprintsRef.current = updatedBlueprints;
			dispatch({ type: 'SET_BLUEPRINTS', payload: updatedBlueprints });
			for (const [entityIdStr, meta] of Object.entries(refs.entityMetaRef.current)) {
				if (meta.blueprintId === bpId) {
					applyScriptsToSingleEntity(Number(entityIdStr));
				}
			}
		} else {
			if (!refs.entityMetaRef.current[id]) {
				refs.entityMetaRef.current[id] = { kind: 'model', path: '', physicsEnabled: false, physicsType: '' };
			}
			applyScriptsToSingleEntity(id);
		}
	};

	const VISUAL_LOGIC_SCRIPT_NAME = 'visual_logic';

	const updateEntityVisualGraph = (
		id: number,
		graph: import('@shared-types').VisualGraphDocument,
		rhaiSource: string,
	) => {
		if (!refs.entityMetaRef.current[id]) {
			refs.entityMetaRef.current[id] = { kind: 'model', path: '', physicsEnabled: false, physicsType: '' };
		}
		const meta = refs.entityMetaRef.current[id];
		meta.visualGraph = graph;
		meta.visualScriptRhai = rhaiSource;

		const existing = meta.scripts ?? [];
		const withoutVisual = existing.filter((s) => s.name !== VISUAL_LOGIC_SCRIPT_NAME);
		const nextScripts = [...withoutVisual, { name: VISUAL_LOGIC_SCRIPT_NAME, source: rhaiSource }];
		meta.scripts = nextScripts;

		sendMotor({
			cmd: 'load_script',
			id,
			path: VISUAL_LOGIC_SCRIPT_NAME,
			source: rhaiSource,
		});
	};

	const applyTransformToEngine = (
		entityId: number,
		patch: Partial<Transform> & {
			positionAxis?: { axis: number; value: number };
			scaleAxis?: { axis: number; value: number };
			rotationEulerDelta?: { axis: number; degrees: number };
			rotationEulerDegrees?: [number, number, number];
		},
		opts?: { bodyRotationOnly?: boolean },
	) => {
		const {
			positionAxis,
			scaleAxis,
			rotationEulerDelta,
			rotationEulerDegrees,
			...transformPatch
		} = patch;
		sendMotor({
			cmd: 'set_transform',
			id: entityId,
			...(positionAxis !== undefined ? { position_axis: positionAxis } : {}),
			...(positionAxis === undefined && transformPatch.position !== undefined
				? { position: transformPatch.position }
				: {}),
			...(transformPatch.rotation !== undefined ? { rotation: transformPatch.rotation } : {}),
			...(scaleAxis !== undefined ? { scale_axis: scaleAxis } : {}),
			...(scaleAxis === undefined && transformPatch.scale !== undefined
				? { scale: transformPatch.scale }
				: {}),
			...(opts?.bodyRotationOnly ? { body_rotation_only: true } : {}),
			...(rotationEulerDelta !== undefined
				? { rotation_euler_delta: rotationEulerDelta }
				: {}),
			...(rotationEulerDegrees !== undefined
				? { rotation_euler_degrees: rotationEulerDegrees }
				: {}),
		});
	};

	const refreshSelectedTransform = (entityId: number) => {
		const meta = refs.entityMetaRef.current[entityId];
		const tr = refs.entityTransformsRef.current[entityId];
		if (!meta || !tr) return;
		dispatch({
			type: 'UPDATE_SELECTED_TRANSFORM',
			payload: {
				entityId,
				position: tr.position,
				rotation: tr.rotation,
				scale: tr.scale,
			},
		});
	};

	/**
	 * Actualiza transform de una entidad. Si pertenece a una blueprint:
	 * - posición: solo la instancia editada
	 * - escala / rotación: blueprint + todas las instancias vinculadas
	 */
	const updateEntityTransform = (
		id: number,
		patch: Partial<{
			position: [number, number, number];
			positionAxis: { axis: number; value: number };
			rotation: [number, number, number, number];
			scale: [number, number, number];
			scaleAxis: { axis: number; value: number };
			bodyRotationOnly?: boolean;
			rotationEulerDelta?: { axis: number; degrees: number };
			rotationEulerDegrees?: [number, number, number];
		}>,
	) => {
		const bodyRotationOnly = patch.bodyRotationOnly;
		const isPlayer = refs.playerEntityIdRef.current === id;
		const isEditorCamera = refs.editorCameraEntityIdRef.current === id;
		const skipOptimisticTransform = isPlayer || isEditorCamera;
		const positionAxis = patch.positionAxis;
		const scaleAxis = patch.scaleAxis;
		const rotationEulerDelta = patch.rotationEulerDelta;
		const rotationEulerDegrees = patch.rotationEulerDegrees;
		const transformPatch: Partial<{
			position: [number, number, number];
			rotation: [number, number, number, number];
			scale: [number, number, number];
		}> = {};
		if (patch.position !== undefined) transformPatch.position = patch.position;
		if (patch.rotation !== undefined) transformPatch.rotation = patch.rotation;
		if (patch.scale !== undefined) transformPatch.scale = patch.scale;
		const motorRotationPatch = {
			...(rotationEulerDelta !== undefined ? { rotationEulerDelta } : {}),
			...(rotationEulerDegrees !== undefined ? { rotationEulerDegrees } : {}),
			...(transformPatch.rotation !== undefined && !skipOptimisticTransform
				? { rotation: transformPatch.rotation }
				: {}),
		};
		const bpId = refs.entityMetaRef.current[id]?.blueprintId;
		const current = refs.entityTransformsRef.current[id]
			?? {
				position: [0, 0, 0] as [number, number, number],
				rotation: [0, 0, 0, 1] as [number, number, number, number],
				scale: [1, 1, 1] as [number, number, number],
			};

		const resolveAxis = (
			vec: [number, number, number],
			axisUpdate?: { axis: number; value: number },
			fullVec?: [number, number, number],
		): [number, number, number] => {
			if (axisUpdate) {
				const next: [number, number, number] = [...vec] as [number, number, number];
				const i = axisUpdate.axis;
				if (i === 0 || i === 1 || i === 2) next[i] = axisUpdate.value;
				return next;
			}
			return fullVec ?? vec;
		};

		const nextForEntity: Transform = {
			position: resolveAxis(current.position, positionAxis, transformPatch.position),
			rotation: transformPatch.rotation ?? current.rotation,
			scale: resolveAxis(current.scale, scaleAxis, transformPatch.scale),
		};

		const engineOpts = bodyRotationOnly ? { bodyRotationOnly: true } : undefined;
		const affectsBlueprintTemplate =
			transformPatch.scale !== undefined
			|| scaleAxis !== undefined
			|| transformPatch.rotation !== undefined
			|| rotationEulerDelta !== undefined
			|| rotationEulerDegrees !== undefined;

		const motorAxisPatch = {
			...(positionAxis !== undefined ? { positionAxis } : {}),
			...(scaleAxis !== undefined ? { scaleAxis } : {}),
		};

		if (!bpId || !affectsBlueprintTemplate) {
			if (!skipOptimisticTransform) {
				refs.entityTransformsRef.current[id] = nextForEntity;
			}
			applyTransformToEngine(
				id,
				{ ...transformPatch, ...motorAxisPatch, ...motorRotationPatch },
				engineOpts,
			);
			if (!skipOptimisticTransform) {
				refreshSelectedTransform(id);
			}
			return;
		}

		const updatedBlueprints = refs.blueprintsRef.current.map((bp) => {
			if (bp.id !== bpId) return bp;
			return {
				...bp,
				...(nextForEntity.scale ? { scale: nextForEntity.scale } : {}),
				...(transformPatch.rotation ? { rotation: transformPatch.rotation } : {}),
			};
		});
		refs.blueprintsRef.current = updatedBlueprints;
		dispatch({ type: 'SET_BLUEPRINTS', payload: updatedBlueprints });

		const bp = updatedBlueprints.find((b) => b.id === bpId);
		const templateScale = bp?.scale ?? nextForEntity.scale;
		const templateRotation = bp?.rotation ?? nextForEntity.rotation;

		for (const [entityIdStr, meta] of Object.entries(refs.entityMetaRef.current)) {
			if (meta.blueprintId !== bpId) continue;
			const entityId = Number(entityIdStr);
			const prev = refs.entityTransformsRef.current[entityId] ?? current;
			const merged: Transform = {
				position:
					entityId === id
						? resolveAxis(prev.position, positionAxis, transformPatch.position)
						: prev.position,
				rotation: templateRotation,
				scale: templateScale,
			};
			refs.entityTransformsRef.current[entityId] = merged;
			const entityPatch =
				entityId === id
					? { ...transformPatch, ...motorAxisPatch }
					: {
						...(transformPatch.scale !== undefined ? { scale: transformPatch.scale } : {}),
						...(transformPatch.rotation !== undefined ? { rotation: transformPatch.rotation } : {}),
					};
			applyTransformToEngine(
				entityId,
				{
					...entityPatch,
					...(entityId === id ? motorRotationPatch : {}),
				},
				entityId === id ? engineOpts : undefined,
			);
		}

		if (!skipOptimisticTransform) {
			refreshSelectedTransform(id);
		}
	};

	const setEntityPhysics = (id: number, enabled: boolean, bodyType: string) => {
		const bpId = refs.entityMetaRef.current[id]?.blueprintId;

		const applyPhysicsToSingleEntity = (entityId: number) => {
			if (!refs.entityMetaRef.current[entityId]) return;
			refs.entityMetaRef.current[entityId].physicsEnabled = enabled;
			refs.entityMetaRef.current[entityId].physicsType = bodyType;
			sendMotor({ cmd: 'set_physics', id: entityId, enabled, body_type: bodyType });
		};

		if (bpId) {
			// Actualizar la blueprint y propagar a todas sus instancias
			const updatedBlueprints = refs.blueprintsRef.current.map((bp) =>
				bp.id === bpId
					? {
						...bp,
						colision: enabled,
						physics_enabled: enabled,
						physics_type: bodyType as import('@shared-types').PhysicsType3D,
					}
					: bp
			);
			refs.blueprintsRef.current = updatedBlueprints;
			dispatch({ type: 'SET_BLUEPRINTS', payload: updatedBlueprints });
			for (const [entityIdStr, meta] of Object.entries(refs.entityMetaRef.current)) {
				if (meta.blueprintId === bpId) {
					applyPhysicsToSingleEntity(Number(entityIdStr));
				}
			}
		} else {
			applyPhysicsToSingleEntity(id);
		}
	};

	const setProjectileConfig = (id: number, speed: number, lifetimeS: number) => {
		const next = {
			speed: Math.max(0, speed),
			lifetime_s: Math.max(0.05, lifetimeS),
		};
		const meta = refs.entityMetaRef.current[id];
		if (meta) {
			meta.projectileConfig = next;
		}
		send3dFn({
			cmd: 'set_projectile_config',
			id,
			speed: next.speed,
			lifetime_s: next.lifetime_s,
		});
	};

	const fireProjectile = (
		templateId: number,
		dir: [number, number, number],
		fromId?: number,
	) => {
		send3dFn({
			cmd: 'fire_projectile',
			template_id: templateId,
			from_id: fromId ?? null,
			dir_x: dir[0],
			dir_y: dir[1],
			dir_z: dir[2],
		});
	};

	const registerPivotEditListener = (fn: (framePath: string, px: number, py: number) => void) => {
		refs.pivotEditListenerRef.current = fn;
	};

	const unregisterPivotEditListener = () => {
		refs.pivotEditListenerRef.current = null;
	};

	const registerQuickBuildClickListener = (
		fn: (
			x: number,
			y: number,
			z: number,
			fitToGrid: boolean,
			scale?: [number, number, number],
		) => void,
	) => {
		refs.quickBuildClickListenerRef.current = fn;
	};

	const unregisterQuickBuildClickListener = () => {
		refs.quickBuildClickListenerRef.current = null;
	};

	const loadSprite = (path: string, name: string) => {
		sendMotor({ cmd: 'load_sprite', path, name });
		dispatch({ type: 'ADD_SPRITE_INFO', payload: { path, name } });
	};

	const removeSprite = (path: string) => {
		sendMotor({ cmd: 'remove_sprite', path });
		dispatch({ type: 'REMOVE_SPRITE_INFO', payload: path });
	};

	const getSpritesList = () => {
		sendMotor({ cmd: 'get_sprites_list' });
	};

	const loadCharacter = (path: string) => {
		sendMotor({ cmd: 'load_character', path });
	};

	const syncEngineUiViewportEdit = (playerId: string | null, menuId: string | null) => {
		dispatch({ type: 'SET_UI_SCREEN_EDITING', payload: { playerId, menuId } });
		if (!supportsPlayerUi) return;
		const active = Boolean(playerId || menuId);
		if (active) {
			const scope = playerId ? 'player' : 'menu';
			const screenId = playerId ?? menuId;
			sendMotor({ cmd: 'set_player_ui_edit_mode', active: true, scope, screen_id: screenId });
		} else {
			sendMotor({ cmd: 'set_player_ui_edit_mode', active: false });
		}
	};

	const addPlayerUiTextBox = (fontPath: string) => {
		if (!supportsPlayerUi) return;
		sendMotor({ cmd: 'add_player_ui_text_box', font_path: fontPath });
	};

	const removePlayerUiTextBox = (id?: number) => {
		if (!supportsPlayerUi) return;
		sendMotor({ cmd: 'remove_player_ui_text_box', ...(id !== undefined ? { id } : {}) });
	};

	const addEditingUiButton = (config: PlayerUiButtonConfig) => {
		if (!supportsPlayerUi) return;
		pendingPlayerUiButtonConfig = config;
		sendMotor({
			cmd: 'add_player_ui_button',
			type: config.type,
			round: config.round,
			backgroundColor: config.backgroundColor,
			texturePath: config.texturePath,
			transparencyBackground: config.transparencyBackground,
			text: config.text,
			textColor: config.textColor,
			transparencyText: config.transparencyText,
			fontPath: config.fontPath,
			fontName: config.fontName,
			borderColor: config.borderColor,
			borderWeight: config.borderWeight,
		});
	};

	const addPlayerUiImage = (imagePath: string) => {
		if (!supportsPlayerUi) return;
		sendMotor({ cmd: 'add_player_ui_image', image_path: imagePath });
	};

	const removePlayerUiImage = (id?: number) => {
		if (!supportsPlayerUi) return;
		sendMotor({ cmd: 'remove_player_ui_image', ...(id !== undefined ? { id } : {}) });
	};

	const removePlayerUiObject = (id?: number) => {
		if (!supportsPlayerUi) return;
		sendMotor({ cmd: 'remove_player_ui_object', ...(id !== undefined ? { id } : {}) });
	};

	const setPlayerUiHudElementProps = (
		elementKind: 'text' | 'button' | 'image' | 'object',
		id: number,
		props: { locked?: boolean; z_index?: number },
	) => {
		if (!supportsPlayerUi) return;
		sendMotor({
			cmd: 'set_player_ui_hud_element_props',
			element_kind: elementKind,
			id,
			...props,
		});
	};

	const setPlayerUiObjectStyle = (
		id: number,
		style: {
			fill_color?: [number, number, number, number];
			texture_path?: string | null;
			clear_texture?: boolean;
			live?: boolean;
			skip_undo?: boolean;
		},
	) => {
		if (!supportsPlayerUi) return;
		sendMotor({
			cmd: 'set_player_ui_object_style',
			id,
			fill_color: style.fill_color,
			texture_path: style.texture_path ?? undefined,
			clear_texture: style.clear_texture ?? false,
			live: style.live ?? false,
			skip_undo: style.skip_undo ?? false,
		});
	};

	const removeEditingUiPlaceholder = (kind: 'button', id: number) => {
		if (kind === 'button' && supportsPlayerUi) {
			sendMotor({ cmd: 'remove_player_ui_button', id });
		}
	};

	const loadHudImage = (path: string, name: string) => {
		sendMotor({ cmd: 'load_hud_image', path, name });
		dispatch({ type: 'ADD_HUD_IMAGE', payload: { path, name } });
	};

	const removeHudImage = (path: string) => {
		sendMotor({ cmd: 'remove_hud_image', path });
		dispatch({ type: 'REMOVE_HUD_IMAGE', payload: path });
	};

	const endUiScreenEdit = () => {
		syncEngineUiViewportEdit(null, null);
	};

	const setPreviewPlaying = (playing: boolean) => {
		dispatch({ type: 'SET_PREVIEW_PLAYING', payload: playing });
		sendMotor({ cmd: 'set_preview_playing', playing });
		if (playing) {
			const playerId = refs.playerEntityIdRef.current;
			if (playerId != null) {
				applyPlayCharacterControlDefaultsIfEmpty(playerId, refs.entityMetaRef, send);
				const bindings = refs.entityMetaRef.current[playerId]?.controlBindings;
				if (bindings) {
					sendMotor({ cmd: 'set_control_bindings', id: playerId, bindings });
				}
			}
		}
		if (supportsPlayerUi) {
			endUiScreenEdit();
		}
	};

	const addUiScreen = (scope: UiScreenScope, name: string): string | null => {
		const trimmed = name.trim();
		if (!trimmed) return null;
		const entry: UiScreenEntry = { id: createUiScreenId(), name: trimmed };
		dispatch({ type: 'ADD_UI_SCREEN', payload: { scope, entry } });
		return entry.id;
	};

	const removeUiScreen = (scope: UiScreenScope, id: string) => {
		dispatch({ type: 'REMOVE_UI_SCREEN', payload: { scope, id } });
	};

	const renameUiScreen = (scope: UiScreenScope, id: string, name: string) => {
		const trimmed = name.trim();
		if (!trimmed) return;
		dispatch({ type: 'RENAME_UI_SCREEN', payload: { scope, id, name: trimmed } });
	};

	const syncPlayerUiScreensToEngine = (screens: UiScreenEntry[]) => {
		if (!supportsPlayerUi) return;
		sendMotor({
			cmd: 'sync_player_ui_screens',
			screens: screens.map((s) => ({
				id: s.id,
				name: s.name,
				active: Boolean(s.active),
			})),
		});
	};

	const setActivePlayerUiScreen = (screenId: string | null) => {
		if (!supportsPlayerUi) return;
		dispatch({ type: 'SET_ACTIVE_PLAYER_UI_SCREEN', payload: screenId });
		sendMotor({
			cmd: 'set_active_player_ui_screen',
			screen_id: screenId,
		});
	};

	const beginUiScreenEdit = (scope: UiScreenScope, id: string) => {
		if (!supportsPlayerUi) return;
		if (scope === 'player') {
			syncEngineUiViewportEdit(id, null);
		} else {
			syncEngineUiViewportEdit(null, id);
		}
	};

	const setDebugMode = (show: boolean) => {
		dispatch({ type: 'SET_DEBUG_MODE', payload: show });
		sendMotor({ cmd: 'set_debug_mode', show });
	};

	const setBackground = (path: string | null) => {
		dispatch({ type: 'SET_BACKGROUND', payload: path });
		if (path) {
			send2dFn({ cmd: 'load_background', path });
		} else {
			send2dFn({ cmd: 'clear_background' });
		}
	};

	const loadSound = (path: string, name: string) => {
		sendMotor({ cmd: 'load_sound', path, name });
		dispatch({ type: 'ADD_SOUND', payload: { path, name } });
	};

	const removeSound = (path: string) => {
		sendMotor({ cmd: 'remove_sound', path });
		dispatch({ type: 'REMOVE_SOUND', payload: path });
	};

	const loadFont = (path: string, name: string) => {
		sendMotor({ cmd: 'load_font', path, name });
		dispatch({ type: 'ADD_FONT', payload: { path, name } });
	};

	const removeFont = (path: string) => {
		sendMotor({ cmd: 'remove_font', path });
		dispatch({ type: 'REMOVE_FONT', payload: path });
	};

	const loadBackgroundToLibrary = (path: string, name: string) => {
		sendMotor({ cmd: 'load_background_asset', path, name });
		dispatch({ type: 'ADD_BACKGROUND', payload: { path, name } });
	};

	const removeBackgroundFromLibrary = (path: string) => {
		sendMotor({ cmd: 'remove_background_asset', path });
		dispatch({ type: 'REMOVE_BACKGROUND', payload: path });
	};

	const registerBlueprintInEngine = (bp: BluePrintEntry) => {
		if (!blueprintUsesModel3D(bp)) return;
		send3dFn({
			cmd: 'register_blueprint',
			blueprint: buildBlueprintPlacementMeta(bp, refs.modelsRef.current),
		});
	};

	const addBlueprint = (entry: BluePrintEntry) => {
		const category = reconcileCategoryWithName(
			normalizeBlueprintCategory(entry.category)
				?? blueprintPlacementCategory(entry, refs.modelsRef.current),
			entry.name,
		);
		const entity_category = blueprintEntityCategoryForEngine(category);
		const normalized: BluePrintEntry = {
			...entry,
			category,
			...(entity_category ? { entity_category } : {}),
		};
		dispatch({ type: 'ADD_BLUEPRINT', payload: normalized });
		registerBlueprintInEngine(normalized);
	};

	const setBlueprints = (entries: BluePrintEntry[]) => {
		const normalized = entries.map((bp) => {
			const category = reconcileCategoryWithName(
				normalizeBlueprintCategory(bp.category)
					?? blueprintPlacementCategory(bp, refs.modelsRef.current),
				bp.name,
			);
			const entity_category = blueprintEntityCategoryForEngine(category);
			return {
				...bp,
				category,
				...(entity_category ? { entity_category } : {}),
			};
		});
		refs.blueprintsRef.current = normalized;
		dispatch({ type: 'SET_BLUEPRINTS', payload: normalized });
		for (const bp of normalized) {
			registerBlueprintInEngine(bp);
		}
	};

	return {
		sendAsync,
		setAnimationPlaying,
		applyInitialAnimationFrame,
		loadModelAsset,
		spawnModel,
		replaceEntityModel,
		removeModelAsset,
		getModelsList,
		retryEngine,
		removeScenario,
		removeCharacter,
		removeEntity,
		setWorldSize,
		setWorldRadius,
		setGridVisible,
		setGridCellSize,
		setGravity,
		setDirectionalLight,
		setTargetFps,
		setGraphicsTextureTier,
		setReflectionTier,
		setReflectionRaytracing,
		setReflectionProbes,
		spawnReflectionProbe,
		setSsrDebugMode,
		setShadowTier,
		setMsaaTier,
		setTaaEnabled,
		setTaaParams,
		setReflectionDebugView,
		setTextureDetailDistance,
		removeCollider,
		removeExecutionArea,
		updateEntityAnimations,
		updateEntityScripts,
		updateEntityVisualGraph,
		setEntityPhysics,
		setProjectileConfig,
		fireProjectile,
		updateEntityTransform,
		registerPivotEditListener,
		unregisterPivotEditListener,
		loadSprite,
		removeSprite,
		getSpritesList,
		loadCharacter,
		setPreviewPlaying,
		addUiScreen,
		removeUiScreen,
		renameUiScreen,
		setActivePlayerUiScreen,
		syncPlayerUiScreensToEngine,
		beginUiScreenEdit,
		endUiScreenEdit,
		addPlayerUiTextBox,
		removePlayerUiTextBox,
		addEditingUiButton,
		addPlayerUiImage,
		removePlayerUiImage,
		removePlayerUiObject,
		setPlayerUiHudElementProps,
		setPlayerUiObjectStyle,
		removeEditingUiPlaceholder,
		loadHudImage,
		removeHudImage,
		setDebugMode,
		setBackground,
		loadSound,
		removeSound,
		loadFont,
		removeFont,
		loadBackgroundToLibrary,
		removeBackgroundFromLibrary,
		addBlueprint,
		setBlueprints,
		registerQuickBuildClickListener,
		unregisterQuickBuildClickListener,
	};
}