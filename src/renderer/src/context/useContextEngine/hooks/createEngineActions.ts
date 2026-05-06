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

	const queuePendingDuplicateRestore = (id: number) => {
		const meta = refs.entityMetaRef.current[id];
		const transform = refs.entityTransformsRef.current[id];
		if (!meta || !transform || !meta.path) return;

		const clonedTransform = cloneTransform(transform);
		clonedTransform.position = [
			clonedTransform.position[0] + 0.5,
			clonedTransform.position[1] + 0.5,
			clonedTransform.position[2],
		];

		const pendingRestore: PendingRestore = {
			transform: clonedTransform,
			physicsEnabled: meta.physicsEnabled ?? false,
			physicsType: meta.physicsType ?? 'static',
			animations: cloneAnimations(meta.animations),
			scripts: cloneScripts(meta.scripts),
		};

		const queue = refs.pendingRestoresRef.current.get(meta.path) ?? [];
		queue.push(pendingRestore);
		refs.pendingRestoresRef.current.set(meta.path, queue);
	};

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

	const duplicateScenario = (id: number) => {
		queuePendingDuplicateRestore(id);
		send({ cmd: 'duplicate_scenario', id });
	};

	const removeCharacter = (id: number) => {
		send({ cmd: 'remove_entity', id });
		dispatch({ type: 'REMOVE_CHARACTER', payload: id });
		if (refs.playerEntityIdRef.current === id) refs.playerEntityIdRef.current = null;
		delete refs.entityMetaRef.current[id];
	};

	const duplicateCharacter = (id: number) => {
		queuePendingDuplicateRestore(id);
		send({ cmd: 'duplicate_character', id });
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
		if (!refs.entityMetaRef.current[id]) {
			refs.entityMetaRef.current[id] = { kind: 'model', path: '', physicsEnabled: false, physicsType: '' };
		}
		refs.entityMetaRef.current[id].animations = animations;
		for (const anim of animations) {
			window.engine.send({
				cmd: 'set_animation',
				id,
				name: anim.name,
				frames: anim.frames,
				fps: anim.fps,
				loop_: anim.loop,
				flip_horizontal: !(anim.facing_right ?? true),
				audio_path: anim.audio_path ?? null,
				logical_w: anim.logical_w ?? 64,
				logical_h: anim.logical_h ?? 64,
				scripts: anim.scripts ?? [],
			} as never);
		}

		const defaultAnim = animations.find((anim) => anim?.is_default) ?? animations[0];
		if (defaultAnim?.name) {
			window.engine.send({ cmd: 'set_default_animation', id, name: defaultAnim.name } as never);
		}
	};

	const updateEntityScripts = (id: number, scripts: EntityScripts) => {
		if (!refs.entityMetaRef.current[id]) {
			refs.entityMetaRef.current[id] = { kind: 'model', path: '', physicsEnabled: false, physicsType: '' };
		}
		refs.entityMetaRef.current[id].scripts = scripts;
	};

	const registerPivotEditListener = (fn: (framePath: string, px: number, py: number) => void) => {
		refs.pivotEditListenerRef.current = fn;
	};

	const unregisterPivotEditListener = () => {
		refs.pivotEditListenerRef.current = null;
	};

	const registerQuickBuildClickListener = (fn: (x: number, y: number, fitToGrid: boolean) => void) => {
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

	const setBackground = (path: string | null) => {
		dispatch({ type: 'SET_BACKGROUND', payload: path });
		if (path) {
			send({ cmd: 'load_background', path });
		} else {
			send({ cmd: 'clear_background' });
		}
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
		duplicateScenario,
		removeCharacter,
		duplicateCharacter,
		setWorldSize,
		setGridVisible,
		setGridCellSize,
		setGravity,
		removeCollider,
		removeExecutionArea,
		updateEntityAnimations,
		updateEntityScripts,
		registerPivotEditListener,
		unregisterPivotEditListener,
		loadSprite,
		removeSprite,
		getSpritesList,
		loadCharacter,
		setPreviewPlaying,
		setBackground,
		addBlueprint,
		setBlueprints,
		registerQuickBuildClickListener,
		unregisterQuickBuildClickListener,
	};
}