import type { Dispatch } from 'react';
import type { BluePrintEntry, EngineAction, EngineInternalRefs, EntityMeta, EntityScripts, Transform } from '../types';
import { buildPlayAnimationFrameCmd } from './applyPendingRestoreToEngine';
import { beginModelReplaceLoading } from './sceneImportOverlay';

interface CreateEngineActionsParams {
	dispatch: Dispatch<EngineAction>
	refs: EngineInternalRefs
	addLog: (text: string, isError?: boolean) => void
	reportBounds: () => void
	send: (cmd: object) => void
}

export function createEngineActions({ dispatch, refs, addLog, reportBounds, send }: CreateEngineActionsParams) {
	const cloneTransform = (transform: Transform): Transform => ({
		position: [...transform.position] as [number, number, number],
		rotation: [...transform.rotation] as [number, number, number, number],
		scale: [...transform.scale] as [number, number, number],
	});

	const cloneAnimations = (animations?: any[]) =>
		animations?.map((anim) => ({
			...anim,
			frames: Array.isArray(anim.frames)
				? anim.frames.map((frame: any) => ({ ...frame }))
				: [],
			scripts: Array.isArray(anim.scripts)
				? anim.scripts.map((script: any) => ({ ...script }))
				: [],
		}));

	const cloneScripts = (scripts?: EntityScripts) =>
		scripts?.map((script) => ({ ...script }));

	const sendAsync = <T,>(cmd: object, waitForEvent: string, onStart?: () => void): Promise<T> => {
		if (onStart) onStart();
		return new Promise((resolve) => {
			refs.pendingEventsRef.current.set(waitForEvent, { resolve });
			window.engine.send(cmd as never);
		});
	};

	const setAnimationPlaying = (entityId: number, playing: boolean) => {
		dispatch({ type: 'SET_ANIMATION_PLAYING', payload: { entityId, playing } });
	};

	const applyInitialAnimationFrame = (entityId: number, animations?: any[]) => {
		if (!animations || animations.length === 0) return;

		const defaultAnim = animations.find((anim) => anim?.is_default);
		if (!defaultAnim) return;
		const firstFrame = defaultAnim.frames?.[0];
		if (!firstFrame?.path) return;

		window.engine.send(
			buildPlayAnimationFrameCmd(entityId, defaultAnim, firstFrame) as never,
		);

		dispatch({ type: 'SET_ANIMATION_PLAYING', payload: { entityId, playing: false } });
	};

	const loadModelAsset = (path: string, name: string) => {
		send({ cmd: 'load_model_asset', path, name });
		dispatch({ type: 'ADD_MODEL_INFO', payload: { path, name, loading: true } });
	};

	const isModelPreloadReady = (path: string): boolean => {
		const entry = refs.modelsRef.current.find((m) => m.path === path);
		return entry != null && entry.loading !== true;
	};

	const removeModelAsset = (path: string) => {
		send({ cmd: 'remove_model_asset', path });
		dispatch({ type: 'REMOVE_MODEL_INFO', payload: path });
	};

	const getModelsList = () => {
		send({ cmd: 'get_models_list' });
	};

	const spawnModel = (path: string, kind: EntityMeta['kind'] = 'model', category?: EntityMeta['entityCategory']) => {
		refs.pendingModelPathRef.current = path;
		refs.pendingSpawnKindRef.current = kind;
		refs.pendingSpawnCategoryRef.current = category ?? null;
		if (!isModelPreloadReady(path)) {
			beginModelReplaceLoading(
				dispatch,
				refs.modelReplaceInProgressRef,
				'entity',
				refs.modelLoadOverlayKindRef,
			);
		}
		send({
			cmd: 'load_model',
			path,
			single_instance: true,
			...(category === 'environment' ? { entity_category: 'environment' } : {}),
		});
	};

	const replaceEntityModel = (entityId: number, modelPath: string) => {
		const meta = refs.entityMetaRef.current[entityId];
		if (meta) {
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
		beginModelReplaceLoading(
			dispatch,
			refs.modelReplaceInProgressRef,
			'model',
			refs.modelLoadOverlayKindRef,
		);
		send({ cmd: 'replace_entity_model', id: entityId, path: modelPath });
	};

	const retryEngine = () => {
		dispatch({ type: 'RESET_ENGINE' });
		addLog('[retry] Reiniciando motor…');
		reportBounds();
	};

	const removeEntity = (id: number) => {
		send({ cmd: 'remove_entity', id });
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
		send({ cmd: 'set_world_size', width, height, depth });
	};

	const setGridVisible = (visible: boolean) => {
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { gridVisible: visible } });
		send({ cmd: 'set_grid_visible', visible });
	};

	const setGridCellSize = (size: number) => {
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { gridCellSize: size } });
		send({ cmd: 'set_grid_cell_size', size });
	};

	const setGravity = (gravity: number) => {
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { gravity } });
		send({ cmd: 'set_gravity', gravity });
	};

	const setDirectionalLight = (settings: {
		ambient?: number;
		intensity?: number;
		shadowDarkness?: number;
	}) => {
		const payload: Partial<import('../types').WorldConfig> = {};
		if (settings.ambient !== undefined) payload.lightAmbient = settings.ambient;
		if (settings.intensity !== undefined) payload.lightIntensity = settings.intensity;
		if (settings.shadowDarkness !== undefined) payload.shadowDarkness = settings.shadowDarkness;
		if (Object.keys(payload).length > 0) {
			dispatch({ type: 'SET_WORLD_CONFIG', payload });
		}
		send({
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
		send({ cmd: 'set_target_fps', fps: normalizedFps });
	};

	const removeCollider = (id: number) => removeEntity(id);
	const removeExecutionArea = (id: number) => removeEntity(id);

	const updateEntityAnimations = (id: number, animations: any[]): any[] => {
		const bpId = refs.entityMetaRef.current[id]?.blueprintId;

		const applyAnimationsToSingleEntity = (entityId: number) => {
			if (!refs.entityMetaRef.current[entityId]) return;
			refs.entityMetaRef.current[entityId].animations = animations;
			for (const anim of animations) {
				window.engine.send({
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
				} as never);
			}
			const defaultAnim = animations.find((anim) => anim?.is_default);
			if (defaultAnim?.name) {
				window.engine.send({ cmd: 'set_default_animation', id: entityId, name: defaultAnim.name } as never);
			} else {
				window.engine.send({ cmd: 'set_default_animation', id: entityId, name: '' } as never);
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
				window.engine.send({ cmd: 'load_script', id: entityId, path: script.name, source: script.source } as never);
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
		window.engine.send({
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
		} as never);
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
		const {
			bodyRotationOnly: _strip,
			positionAxis,
			scaleAxis,
			rotationEulerDelta,
			rotationEulerDegrees,
			...transformPatch
		} = patch;
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
			window.engine.send({ cmd: 'set_physics', id: entityId, enabled, body_type: bodyType } as never);
		};

		if (bpId) {
			// Actualizar la blueprint y propagar a todas sus instancias
			const updatedBlueprints = refs.blueprintsRef.current.map((bp) =>
				bp.id === bpId ? { ...bp, physics_enabled: enabled, physics_type: bodyType } : bp
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
		send({ cmd: 'load_sprite', path, name });
		dispatch({ type: 'ADD_SPRITE_INFO', payload: { path, name } });
	};

	const removeSprite = (path: string) => {
		send({ cmd: 'remove_sprite', path });
		dispatch({ type: 'REMOVE_SPRITE_INFO', payload: path });
	};

	const getSpritesList = () => {
		send({ cmd: 'get_sprites_list' });
	};

	const loadCharacter = (path: string) => {
		send({ cmd: 'load_character', path });
	};

	const setPreviewPlaying = (playing: boolean) => {
		dispatch({ type: 'SET_PREVIEW_PLAYING', payload: playing });
		send({ cmd: 'set_preview_playing', playing });
	};

	const setDebugMode = (show: boolean) => {
		dispatch({ type: 'SET_DEBUG_MODE', payload: show });
		send({ cmd: 'set_debug_mode', show });
	};

	const setBackground = (path: string | null) => {
		dispatch({ type: 'SET_BACKGROUND', payload: path });
		if (path) {
			send({ cmd: 'load_background', path });
		} else {
			send({ cmd: 'clear_background' });
		}
	};

	const loadSound = (path: string, name: string) => {
		send({ cmd: 'load_sound', path, name });
		dispatch({ type: 'ADD_SOUND', payload: { path, name } });
	};

	const removeSound = (path: string) => {
		send({ cmd: 'remove_sound', path });
		dispatch({ type: 'REMOVE_SOUND', payload: path });
	};

	const loadBackgroundToLibrary = (path: string, name: string) => {
		send({ cmd: 'load_background_asset', path, name });
		dispatch({ type: 'ADD_BACKGROUND', payload: { path, name } });
	};

	const removeBackgroundFromLibrary = (path: string) => {
		send({ cmd: 'remove_background_asset', path });
		dispatch({ type: 'REMOVE_BACKGROUND', payload: path });
	};

	const addBlueprint = (entry: BluePrintEntry) => {
		dispatch({ type: 'ADD_BLUEPRINT', payload: entry });
	};

	const setBlueprints = (entries: BluePrintEntry[]) => {
		dispatch({ type: 'SET_BLUEPRINTS', payload: entries });
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
		setGridVisible,
		setGridCellSize,
		setGravity,
		setDirectionalLight,
		setTargetFps,
		removeCollider,
		removeExecutionArea,
		updateEntityAnimations,
		updateEntityScripts,
		setEntityPhysics,
		updateEntityTransform,
		registerPivotEditListener,
		unregisterPivotEditListener,
		loadSprite,
		removeSprite,
		getSpritesList,
		loadCharacter,
		setPreviewPlaying,
		setDebugMode,
		setBackground,
		loadSound,
		removeSound,
		loadBackgroundToLibrary,
		removeBackgroundFromLibrary,
		addBlueprint,
		setBlueprints,
		registerQuickBuildClickListener,
		unregisterQuickBuildClickListener,
	};
}