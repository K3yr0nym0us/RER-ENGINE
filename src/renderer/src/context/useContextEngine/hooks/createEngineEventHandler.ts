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
import type { GameStyle } from '@shared-types';
import { FIRST_PERSON_PLAYER_BODY_SCALE, isEditorBoxPath, isPlayerPath } from '@shared-types';
import { applyFirstPersonControlDefaultsIfEmpty } from '../../../defaults/applyFirstPersonControlDefaults';
import {
	applySavedFirstPersonView,
	ensureFirstPersonPlayerOnLoad,
} from '../../../defaults/firstPersonSceneRestore';
import { setSceneCommandForSavedProject } from '../../../defaults/projectSceneLoad';
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
	'preview_playing_changed',
]);

interface CreateEngineEventHandlerParams {
	dispatch: Dispatch<EngineAction>
	refs: EngineInternalRefs
	addLog: (text: string, isError?: boolean) => void
	projectType?: string
	gameStyle?: GameStyle
	applyInitialAnimationFrame: (entityId: number, animations?: any[]) => void
	setLocale?: (locale: Locale) => void
}

function applyFirstPersonDefaultsForPlayer(
	characterId: number,
	gameStyle: GameStyle | undefined,
	refs: EngineInternalRefs,
) {
	if (gameStyle !== 'first-person') return;
	// Proyecto abierto desde .save: bindings y entidades vienen del archivo, no de la plantilla.
	if (refs.initialSaveRef.current) return;

	applyFirstPersonControlDefaultsIfEmpty(characterId, refs.entityMetaRef, (cmd) => {
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
					}
					: baseSave;

				refs.initialSaveRef.current = save;
				if (gameStyle === 'first-person' && projectType === '3D' && save.playerTransform) {
					refs.pendingFirstPersonViewRef.current = save.playerTransform;
					refs.firstPersonViewRef.current = save.playerTransform;
				}

				if (save.world) {
					savedGravity = save.world.gravity;
					dispatch({ type: 'SET_WORLD_CONFIG', payload: save.world });
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
				}
			if (save.language && (save.language === 'en' || save.language === 'es')) {
				const validLocale = save.language as Locale;
				setLocale?.(validLocale);
			}
			if (save.camera2d) {
				sendEngine({ cmd: 'set_camera2d', x: save.camera2d.x, y: save.camera2d.y, half_h: save.camera2d.halfH } as never);
				refs.camera2dRef.current = save.camera2d;
			}
				if (save.sprites && save.sprites.length > 0) {
					for (const sprite of save.sprites) {
						sendEngine({ cmd: 'load_sprite', path: sprite.path, name: sprite.name } as never);
						dispatch({ type: 'ADD_SPRITE_INFO', payload: { path: sprite.path, name: sprite.name } });
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
				if (save.backgroundPath) {
					sendEngine({ cmd: 'load_background', path: save.backgroundPath } as never);
				}
				for (const entity of save.entities) {
					const transform: Transform = {
						position: entity.position,
						rotation: entity.rotation,
						scale: entity.scale,
					};
					if (entity.kind === 'collider' && entity.points) {
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
						const playerTransform: Transform = savedPlayer
							? {
								position: savedPlayer.position,
								rotation: entity.rotation,
								scale: FIRST_PERSON_PLAYER_BODY_SCALE,
							}
							: transform;
						const pendingRestore: PendingRestore = {
							transform: playerTransform,
							name: entity.name,
							physicsEnabled: entity.physics_enabled ?? false,
							physicsType: entity.physics_type ?? 'static',
							scripts: entity.scripts,
							controlBindings: entity.control_bindings,
						};
						const queue = refs.pendingRestoresRef.current.get('[Player]') ?? [];
						queue.push(pendingRestore);
						refs.pendingRestoresRef.current.set('[Player]', queue);
						sendEngine({ cmd: 'load_character', path: entity.path } as never);
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
						};
						const queue = refs.pendingRestoresRef.current.get(entity.path) ?? [];
						queue.push(pendingRestore);
						refs.pendingRestoresRef.current.set(entity.path, queue);
						if (entity.kind === 'scenario') sendEngine({ cmd: 'load_scenario', path: entity.path } as never);
						if (entity.kind === 'character') sendEngine({ cmd: 'load_character', path: entity.path } as never);
						if (entity.kind === 'model' && entity.path && !isEditorBoxPath(entity.path)) {
							sendEngine({ cmd: 'load_model', path: entity.path } as never);
						}
					}
				}
				if (gameStyle === 'first-person' && projectType === '3D') {
					ensureFirstPersonPlayerOnLoad(save, refs.pendingRestoresRef, (cmd) => sendEngine(cmd as never));
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
				if (pending.name && pending.name.trim().length > 0) {
					window.engine.send({ cmd: 'set_entity_name', id, name: pending.name, force: true } as never);
				}
				if (pending.scripts) {
					refs.entityMetaRef.current[id].scripts = pending.scripts;
					for (const script of pending.scripts) {
						window.engine.send({ cmd: 'load_script', id, path: script.name, source: script.source } as never);
					}
				}
				if (pending.controlBindings) {
					refs.entityMetaRef.current[id].controlBindings = pending.controlBindings;
					window.engine.send({ cmd: 'set_control_bindings', id, bindings: pending.controlBindings } as never);
				}
				if (pending.blueprintId) {
					refs.entityMetaRef.current[id].blueprintId = pending.blueprintId;
				}
				refs.entityTransformsRef.current[id] = pending.transform;
				if (editorBoxQueue.length === 0) refs.pendingRestoresRef.current.delete('[EditorBox]');
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

		if (event.event === 'entity_selected') {
			const selected = event as unknown as EntitySelected;
			refs.entityTransformsRef.current[selected.id] = { position: selected.position, rotation: selected.rotation, scale: selected.scale };
			if (refs.entityMetaRef.current[selected.id]) {
				refs.entityMetaRef.current[selected.id].name = selected.name;
				refs.entityMetaRef.current[selected.id].physicsEnabled = selected.physics_enabled ?? false;
				refs.entityMetaRef.current[selected.id].physicsType = selected.physics_type ?? '';
			}
			const meta = refs.entityMetaRef.current[selected.id];
			dispatch({
				type: 'SELECT_ENTITY',
				payload: {
					id: selected.id,
					name: selected.name,
					position: selected.position,
					rotation: selected.rotation,
					scale: selected.scale,
					physicsEnabled: selected.physics_enabled ?? false,
					physicsType: selected.physics_type ?? '',
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

		if (event.event === 'scenario_loaded') {
			const scenario = event as unknown as ScenarioLoaded;
			dispatch({ type: 'ADD_SCENARIO', payload: { id: scenario.id, path: scenario.path } });
			refs.entityMetaRef.current[scenario.id] = { kind: 'scenario', path: scenario.path, physicsEnabled: false, physicsType: '' };
			const queue = refs.pendingRestoresRef.current.get(scenario.path);
			if (queue && queue.length > 0) {
				const pending = queue.shift()!;
				const pendingTransform = pending.transform;
				if (pending.name && pending.name.trim().length > 0) {
					refs.entityMetaRef.current[scenario.id].name = pending.name;
					window.engine.send({ cmd: 'set_entity_name', id: scenario.id, name: pending.name, force: true } as never);
				}
				window.engine.send({ cmd: 'set_transform', id: scenario.id, position: pendingTransform.position, rotation: pendingTransform.rotation, scale: pendingTransform.scale, track_undo: false } as never);
				if (pending.physicsEnabled) {
					window.engine.send({ cmd: 'set_physics', id: scenario.id, enabled: true, body_type: pending.physicsType } as never);
					refs.entityMetaRef.current[scenario.id].physicsEnabled = true;
					refs.entityMetaRef.current[scenario.id].physicsType = pending.physicsType;
				}
				if (pending.animations) {
					refs.entityMetaRef.current[scenario.id].animations = pending.animations;
					for (const anim of pending.animations) {
						window.engine.send({
							cmd: 'set_animation',
							id: scenario.id,
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
					const defaultAnim = pending.animations.find((anim: any) => anim?.is_default) ?? pending.animations[0];
					if (defaultAnim?.name) {
						window.engine.send({ cmd: 'set_default_animation', id: scenario.id, name: defaultAnim.name } as never);
					}
					// Si la entidad usa una animación de un solo frame (objeto estático),
					// aplicar el primer frame de inmediato para que quede visible al cargar.
					applyInitialAnimationFrame(scenario.id, pending.animations);
				}
				if (pending.scripts) {
					refs.entityMetaRef.current[scenario.id].scripts = pending.scripts;
					for (const script of pending.scripts) {
						window.engine.send({ cmd: 'load_script', id: scenario.id, path: script.name, source: script.source } as never);
					}
				}
				if (pending.controlBindings) {
					refs.entityMetaRef.current[scenario.id].controlBindings = pending.controlBindings;
					window.engine.send({ cmd: 'set_control_bindings', id: scenario.id, bindings: pending.controlBindings } as never);
				}
				if (pending.blueprintId) {
					refs.entityMetaRef.current[scenario.id].blueprintId = pending.blueprintId;
				}
				refs.entityTransformsRef.current[scenario.id] = pendingTransform;
				if (queue.length === 0) refs.pendingRestoresRef.current.delete(scenario.path);
			}
		}

		if (event.event === 'character_loaded') {
			const character = event as unknown as CharacterLoaded;
			const applyPendingRestore = (id: number, path: string) => {
				const queue = refs.pendingRestoresRef.current.get(path);
				if (!queue || queue.length === 0) return;

				const pending = queue.shift()!;
				const pendingTransform = pending.transform;
				if (pending.name && pending.name.trim().length > 0) {
					if (refs.entityMetaRef.current[id]) {
						refs.entityMetaRef.current[id].name = pending.name;
					}
					window.engine.send({ cmd: 'set_entity_name', id, name: pending.name, force: true } as never);
				}
				const isPlayer = isPlayerPath(path);
				window.engine.send({
					cmd: 'set_transform',
					id,
					position: pendingTransform.position,
					rotation: pendingTransform.rotation,
					...(isPlayer ? {} : { scale: pendingTransform.scale }),
					track_undo: false,
				} as never);

				if (pending.physicsEnabled) {
					window.engine.send({ cmd: 'set_physics', id, enabled: true, body_type: pending.physicsType } as never);
					if (refs.entityMetaRef.current[id]) {
						refs.entityMetaRef.current[id].physicsEnabled = true;
						refs.entityMetaRef.current[id].physicsType = pending.physicsType;
					}
				}

				if (pending.animations) {
					if (refs.entityMetaRef.current[id]) {
						refs.entityMetaRef.current[id].animations = pending.animations;
					}
					for (const anim of pending.animations) {
						window.engine.send({
							cmd: 'set_animation',
							id,
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
					const defaultAnim = pending.animations.find((anim: any) => anim?.is_default) ?? pending.animations[0];
					if (defaultAnim?.name) {
						window.engine.send({ cmd: 'set_default_animation', id, name: defaultAnim.name } as never);
					}
					applyInitialAnimationFrame(id, pending.animations);
				}

				if (pending.scripts) {
					if (refs.entityMetaRef.current[id]) {
						refs.entityMetaRef.current[id].scripts = pending.scripts;
					}
					for (const script of pending.scripts) {
						window.engine.send({ cmd: 'load_script', id, path: script.name, source: script.source } as never);
					}
				}

				if (pending.controlBindings) {
					if (refs.entityMetaRef.current[id]) {
						refs.entityMetaRef.current[id].controlBindings = pending.controlBindings;
					}
					window.engine.send({ cmd: 'set_control_bindings', id, bindings: pending.controlBindings } as never);
				}

				if (pending.blueprintId) {
					if (refs.entityMetaRef.current[id]) {
						refs.entityMetaRef.current[id].blueprintId = pending.blueprintId;
					}
				}

				refs.entityTransformsRef.current[id] = pendingTransform;

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
							physicsEnabled: false,
							physicsType: '',
						};
					}
					applyPendingRestore(character.id, character.path);
					applySavedFirstPersonView(
						refs.pendingFirstPersonViewRef.current,
						character.id,
						refs.entityTransformsRef,
						{ editorOrbit: true },
					);
					refs.pendingFirstPersonViewRef.current = null;
					const feet = refs.entityTransformsRef.current[character.id]?.position
						?? refs.firstPersonViewRef.current?.position;
					if (feet) {
						refs.entityTransformsRef.current[character.id] = {
							position: feet,
							rotation: [0, 0, 0, 1],
							scale: FIRST_PERSON_PLAYER_BODY_SCALE,
						};
					}
					applyFirstPersonDefaultsForPlayer(character.id, gameStyle, refs);
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
		}

		if (event.event === 'sprite_loaded') {
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
			dispatch({ type: 'ENGINE_STOPPED', payload: (event as { code?: number }).code });
		}

		if (event.event === 'preview_playing_changed') {
			const playing = Boolean((event as { playing?: boolean }).playing);
			dispatch({ type: 'SET_PREVIEW_PLAYING', payload: playing });
			if (
				playing
				&& gameStyle === 'first-person'
				&& projectType === '3D'
				&& refs.firstPersonViewRef.current
			) {
				applySavedFirstPersonView(
					refs.firstPersonViewRef.current,
					refs.playerEntityIdRef.current,
					refs.entityTransformsRef,
					{ editorOrbit: false },
				);
			}
		}

		if (event.event === 'error') {
			dispatch({ type: 'SET_ERROR', payload: (event as { message?: string }).message ?? 'Error desconocido' });
		}

		if (event.event === 'drawing_progress') {
			dispatch({ type: 'SET_TOOL_PROGRESS', payload: (event as { count?: number }).count ?? 0 });
		}

		if (event.event === 'collider_created') {
			const collider = event as { id?: number; points?: [[number, number], [number, number], [number, number], [number, number]] };
			const id = collider.id ?? -1;
			refs.entityMetaRef.current[id] = { kind: 'collider', path: '[Colisionador]', physicsEnabled: true, physicsType: 'static', points: collider.points };
			const transformFromPoints = buildTransformFromPoints(collider.points);
			if (transformFromPoints) {
				refs.entityTransformsRef.current[id] = transformFromPoints;
			}
			dispatch({ type: 'ADD_COLLIDER', payload: { id, path: '[Colisionador]' } });
			dispatch({ type: 'SET_TOOL_PROGRESS', payload: null });
		}

		if (event.event === 'execution_area_created') {
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
				window.engine.send({ cmd: 'set_transform', id, position: pending.transform.position, rotation: pending.transform.rotation, scale: pending.transform.scale, track_undo: false } as never);
				refs.entityTransformsRef.current[id] = pending.transform;
				if (pending.name && pending.name.trim().length > 0) {
					refs.entityMetaRef.current[id].name = pending.name;
					window.engine.send({ cmd: 'set_entity_name', id, name: pending.name, force: true } as never);
				}
				if (pending.scripts) {
					refs.entityMetaRef.current[id].scripts = pending.scripts;
					for (const script of pending.scripts) {
						window.engine.send({ cmd: 'load_script', id, path: script.name, source: script.source } as never);
					}
				}
				if (queue.length === 0) refs.pendingRestoresRef.current.delete('[ExecutionArea]');
			}
			dispatch({ type: 'ADD_EXECUTION_AREA', payload: { id, path: '[ExecutionArea]' } });
			dispatch({ type: 'SET_TOOL_PROGRESS', payload: null });
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
			meta.animations = meta.animations.map((anim: any) =>
				anim?.name === e.name
					? { ...anim, logical_w: e.logical_w, logical_h: e.logical_h }
					: anim,
			);
		}

		if (event.event === 'entity_removed') {
			const e = event as unknown as { id: number; kind?: 'scenario' | 'character' | 'model' | 'collider' | 'execution_area' };
			const removedKind = e.kind ?? refs.entityMetaRef.current[e.id]?.kind;
			if (removedKind === 'scenario') {
				dispatch({ type: 'REMOVE_SCENARIO', payload: e.id });
			} else if (removedKind === 'character') {
				dispatch({ type: 'REMOVE_CHARACTER', payload: e.id });
			} else if (removedKind === 'collider') {
				dispatch({ type: 'REMOVE_COLLIDER', payload: e.id });
			} else if (removedKind === 'execution_area') {
				dispatch({ type: 'REMOVE_EXECUTION_AREA', payload: e.id });
			}
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
			if (
				gameStyle === 'first-person'
				&& projectType === '3D'
				&& metrics.first_person_position
				&& metrics.first_person_yaw != null
				&& metrics.first_person_pitch != null
			) {
				refs.firstPersonViewRef.current = {
					position: metrics.first_person_position,
					scale: FIRST_PERSON_PLAYER_BODY_SCALE,
					yaw: metrics.first_person_yaw,
					pitch: metrics.first_person_pitch,
				};
				const playerId = refs.playerEntityIdRef.current;
				if (playerId != null) {
					refs.entityTransformsRef.current[playerId] = {
						position: metrics.first_person_position,
						rotation: [0, 0, 0, 1],
						scale: FIRST_PERSON_PLAYER_BODY_SCALE,
					};
				}
			}
		}
	};
}