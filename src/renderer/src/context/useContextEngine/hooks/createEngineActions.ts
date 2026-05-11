import type { Dispatch } from 'react';
import type { BluePrintEntry, EngineAction, EngineInternalRefs, EntityScripts, PendingRestore, Transform } from '../types';

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

		const firstAnim = animations[0];
		const firstFrame = firstAnim?.frames?.[0];
		if (!firstFrame?.path) return;

		const fallbackW = firstAnim.logical_w ?? 64;
		const fallbackH = firstAnim.logical_h ?? 64;
		const pivotX = firstFrame.pivot_x ?? Math.round((firstFrame.src_w ?? fallbackW) / 2);
		const pivotY = firstFrame.pivot_y ?? (firstFrame.src_h ?? fallbackH);

		window.engine.send({
			cmd: 'play_animation_frame',
			id: entityId,
			path: firstFrame.path,
			pivot_x: pivotX,
			pivot_y: pivotY,
			logical_w: fallbackW,
			logical_h: fallbackH,
			src_x: firstFrame.src_x,
			src_y: firstFrame.src_y,
			src_w: firstFrame.src_w,
			src_h: firstFrame.src_h,
		} as never);

		dispatch({ type: 'SET_ANIMATION_PLAYING', payload: { entityId, playing: false } });
	};

	const loadModel = (path: string) => {
		dispatch({ type: 'CLEAR_ENTITIES' });
		send({ cmd: 'load_model', path });
	};

	const retryEngine = () => {
		dispatch({ type: 'RESET_ENGINE' });
		addLog('[retry] Reiniciando motor…');
		reportBounds();
	};

	const removeScenario = (id: number) => {
		send({ cmd: 'remove_entity', id });
		dispatch({ type: 'REMOVE_SCENARIO', payload: id });
		delete refs.entityMetaRef.current[id];
	};

	const removeCharacter = (id: number) => {
		send({ cmd: 'remove_entity', id });
		dispatch({ type: 'REMOVE_CHARACTER', payload: id });
		if (refs.playerEntityIdRef.current === id) refs.playerEntityIdRef.current = null;
		delete refs.entityMetaRef.current[id];
	};

	const setWorldSize = (width: number, height: number) => {
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { worldWidth: width, worldHeight: height } });
		send({ cmd: 'set_world_size', width, height });
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

	const setTargetFps = (fps: number) => {
		const parsedFps = Number.isFinite(fps) ? fps : 60;
		const normalizedFps = Math.max(1, Math.min(1000, Math.round(parsedFps)));
		dispatch({ type: 'SET_WORLD_CONFIG', payload: { targetFps: normalizedFps } });
		send({ cmd: 'set_target_fps', fps: normalizedFps });
	};

	const removeCollider = (id: number) => {
		send({ cmd: 'remove_entity', id });
		dispatch({ type: 'REMOVE_COLLIDER', payload: id });
		delete refs.entityMetaRef.current[id];
	};

	const removeExecutionArea = (id: number) => {
		send({ cmd: 'remove_entity', id });
		dispatch({ type: 'REMOVE_EXECUTION_AREA', payload: id });
		delete refs.entityMetaRef.current[id];
	};

	const updateEntityAnimations = (id: number, animations: any[]) => {
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
					logical_w: anim.logical_w,
					logical_h: anim.logical_h,
					scripts: anim.scripts ?? [],
					is_cancelable: anim.is_cancelable ?? true,
				} as never);
			}
			const defaultAnim = animations.find((anim) => anim?.is_default) ?? animations[0];
			if (defaultAnim?.name) {
				window.engine.send({ cmd: 'set_default_animation', id: entityId, name: defaultAnim.name } as never);
			}
		};

		if (bpId) {
			// Actualizar la blueprint y propagar a todas sus instancias
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

	const registerQuickBuildClickListener = (fn: (x: number, y: number, fitToGrid: boolean, scale?: [number, number, number]) => void) => {
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
		loadModel,
		retryEngine,
		removeScenario,
		removeCharacter,
		setWorldSize,
		setGridVisible,
		setGridCellSize,
		setGravity,
		setTargetFps,
		removeCollider,
		removeExecutionArea,
		updateEntityAnimations,
		updateEntityScripts,
		setEntityPhysics,
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