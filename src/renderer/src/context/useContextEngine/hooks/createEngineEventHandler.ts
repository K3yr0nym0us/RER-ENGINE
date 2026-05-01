import type { Dispatch } from 'react';
import type {
	AnimationFinished,
	Camera2dUpdated,
	CharacterLoaded,
	EngineEvent,
	EntitySelected,
	PhysicsChanged,
	PivotSelected,
	PlayerReady,
	ScenarioLoaded,
	SpriteLoaded,
	SpriteRemoved,
	SpritesList,
} from '@shared-types';
import type { EngineAction, EngineInternalRefs, PendingRestore, Transform } from '../types';

interface CreateEngineEventHandlerParams {
	dispatch: Dispatch<EngineAction>
	refs: EngineInternalRefs
	addLog: (text: string, isError?: boolean) => void
	projectType?: string
	applyInitialAnimationFrame: (entityId: number, animations?: any[]) => void
}

export function createEngineEventHandler({
	dispatch,
	refs,
	addLog,
	projectType,
	applyInitialAnimationFrame,
}: CreateEngineEventHandlerParams) {
	return (event: EngineEvent) => {
		addLog(JSON.stringify(event), event.event === 'error');

		const pendingEvent = refs.pendingEventsRef.current.get(event.event);
		if (pendingEvent) {
			pendingEvent.resolve(event);
			refs.pendingEventsRef.current.delete(event.event);
		}

		if (event.event === 'ready') {
			dispatch({ type: 'SET_READY' });
			if (refs.readyTimer.current) clearTimeout(refs.readyTimer.current);
			if (projectType) {
				window.engine.send({ cmd: 'set_scene', scene: projectType } as never);
			}
			refs.mainPlayerHandled.current = false;
			refs.playerRemoved.current = false;
			refs.pendingPlayerDups.current = [];
			refs.pendingDupQ.current = [];
			const sendEngine = window.engine.send;
			const save = refs.initialSaveRef.current;
			if (save) {
				if (save.world) {
					dispatch({ type: 'SET_WORLD_CONFIG', payload: save.world });
					sendEngine({ cmd: 'set_world_size', width: save.world.worldWidth, height: save.world.worldHeight } as never);
					sendEngine({ cmd: 'set_grid_visible', visible: save.world.gridVisible } as never);
					sendEngine({ cmd: 'set_grid_cell_size', size: save.world.gridCellSize } as never);
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
						sendEngine({ cmd: 'create_collider_from_points', points: entity.points } as never);
					} else if (entity.kind === 'character' && entity.path === '[Player]') {
						refs.pendingPlayerDups.current.push(transform);
					} else {
						const pendingRestore: PendingRestore = {
							transform,
							name: entity.name,
							physicsEnabled: entity.physics_enabled ?? false,
							physicsType: entity.physics_type ?? 'static',
							animations: entity.animations,
							scripts: entity.scripts,
						};
						const queue = refs.pendingRestoresRef.current.get(entity.path) ?? [];
						queue.push(pendingRestore);
						refs.pendingRestoresRef.current.set(entity.path, queue);
						if (entity.kind === 'scenario') sendEngine({ cmd: 'load_scenario', path: entity.path } as never);
						if (entity.kind === 'character') sendEngine({ cmd: 'load_character', path: entity.path } as never);
						if (entity.kind === 'model') sendEngine({ cmd: 'load_model', path: entity.path } as never);
					}
				}
			}
		}

		if (event.event === 'model_loaded') {
			const id = (event as { id?: number }).id ?? -1;
			dispatch({ type: 'ADD_ENTITY', payload: id });
		}

		if (event.event === 'entity_selected') {
			const selected = event as EntitySelected;
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

		if (event.event === 'player_ready') {
			const playerReady = event as PlayerReady;
			refs.playerEntityIdRef.current = playerReady.id;
			refs.entityTransformsRef.current[playerReady.id] = {
				position: playerReady.position,
				rotation: [0, 0, 0, 1],
				scale: playerReady.scale,
			};
			refs.entityMetaRef.current[playerReady.id] = { kind: 'character', path: '[Player]', physicsEnabled: false, physicsType: '' };
			const save = refs.initialSaveRef.current;
			if (save != null && save.playerTransform === null) {
				window.engine.send({ cmd: 'remove_entity', id: playerReady.id } as never);
				refs.playerEntityIdRef.current = null;
				refs.playerRemoved.current = true;
				delete refs.entityMetaRef.current[playerReady.id];
			} else if (save?.playerTransform) {
				window.engine.send({
					cmd: 'set_transform',
					id: playerReady.id,
					position: save.playerTransform.position,
					scale: save.playerTransform.scale,
				} as never);
				refs.entityTransformsRef.current[playerReady.id] = {
					position: save.playerTransform.position,
					rotation: [0, 0, 0, 1],
					scale: save.playerTransform.scale,
				};
				for (const duplicateTransform of refs.pendingPlayerDups.current) {
					refs.pendingDupQ.current.push(duplicateTransform);
					window.engine.send({ cmd: 'duplicate_character', id: playerReady.id } as never);
				}
				refs.pendingPlayerDups.current = [];
			}
		}

		if (event.event === 'camera_2d_updated') {
			const cameraUpdated = event as Camera2dUpdated;
			refs.camera2dRef.current = { x: cameraUpdated.x, y: cameraUpdated.y, halfH: cameraUpdated.half_h };
		}

		if (event.event === 'background_loaded') {
			dispatch({ type: 'SET_BACKGROUND', payload: (event as { path?: string }).path ?? null });
		}

		if (event.event === 'scenario_loaded') {
			const scenario = event as ScenarioLoaded;
			dispatch({ type: 'ADD_SCENARIO', payload: { id: scenario.id, path: scenario.path } });
			refs.entityMetaRef.current[scenario.id] = { kind: 'scenario', path: scenario.path, physicsEnabled: false, physicsType: '' };
			const queue = refs.pendingRestoresRef.current.get(scenario.path);
			if (queue && queue.length > 0) {
				const pending = queue.shift()!;
				if (pending.name && pending.name.trim().length > 0) {
					refs.entityMetaRef.current[scenario.id].name = pending.name;
					window.engine.send({ cmd: 'set_entity_name', id: scenario.id, name: pending.name } as never);
				}
				window.engine.send({ cmd: 'set_transform', id: scenario.id, position: pending.transform.position, rotation: pending.transform.rotation, scale: pending.transform.scale } as never);
				refs.entityTransformsRef.current[scenario.id] = pending.transform;
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
							audio_path: anim.audio_path ?? null,
							logical_w: anim.logical_w ?? 64,
							logical_h: anim.logical_h ?? 64,
							scripts: anim.scripts ?? [],
						} as never);
					}
				}
				if (pending.scripts) {
					refs.entityMetaRef.current[scenario.id].scripts = pending.scripts;
					for (const script of pending.scripts) {
						window.engine.send({ cmd: 'load_script', id: scenario.id, path: script.name, source: script.source } as never);
					}
				}
				if (queue.length === 0) refs.pendingRestoresRef.current.delete(scenario.path);
			}
		}

		if (event.event === 'character_loaded') {
			const character = event as CharacterLoaded;
			if (character.path === '[Player]') {
				if (!refs.mainPlayerHandled.current) {
					refs.mainPlayerHandled.current = true;
					if (!refs.playerRemoved.current) {
						dispatch({ type: 'ADD_CHARACTER', payload: { id: character.id, path: character.path } });
					}
					refs.playerRemoved.current = false;
				} else {
					dispatch({ type: 'ADD_CHARACTER', payload: { id: character.id, path: character.path } });
					refs.entityMetaRef.current[character.id] = { kind: 'character', path: '[Player]', physicsEnabled: false, physicsType: '' };
					const duplicateTransform = refs.pendingDupQ.current.shift();
					if (duplicateTransform) {
						window.engine.send({ cmd: 'set_transform', id: character.id, position: duplicateTransform.position, rotation: duplicateTransform.rotation, scale: duplicateTransform.scale } as never);
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
				const queue = refs.pendingRestoresRef.current.get(character.path);
				if (queue && queue.length > 0) {
					const pending = queue.shift()!;
					if (pending.name && pending.name.trim().length > 0) {
						refs.entityMetaRef.current[character.id].name = pending.name;
						window.engine.send({ cmd: 'set_entity_name', id: character.id, name: pending.name } as never);
					}
					window.engine.send({ cmd: 'set_transform', id: character.id, position: pending.transform.position, rotation: pending.transform.rotation, scale: pending.transform.scale } as never);
					refs.entityTransformsRef.current[character.id] = pending.transform;
					if (pending.physicsEnabled) {
						window.engine.send({ cmd: 'set_physics', id: character.id, enabled: true, body_type: pending.physicsType } as never);
						refs.entityMetaRef.current[character.id].physicsEnabled = true;
						refs.entityMetaRef.current[character.id].physicsType = pending.physicsType;
					}
					if (pending.animations) {
						refs.entityMetaRef.current[character.id].animations = pending.animations;
						for (const anim of pending.animations) {
							window.engine.send({
								cmd: 'set_animation',
								id: character.id,
								name: anim.name,
								frames: anim.frames,
								fps: anim.fps,
								loop_: anim.loop,
								audio_path: anim.audio_path ?? null,
								logical_w: anim.logical_w ?? 64,
								logical_h: anim.logical_h ?? 64,
								scripts: anim.scripts ?? [],
							} as never);
						}
						applyInitialAnimationFrame(character.id, pending.animations);
						applyInitialAnimationFrame(character.id, pending.animations);
					}
					if (pending.scripts) {
						refs.entityMetaRef.current[character.id].scripts = pending.scripts;
						for (const script of pending.scripts) {
							window.engine.send({ cmd: 'load_script', id: character.id, path: script.name, source: script.source } as never);
						}
					}
					if (queue.length === 0) refs.pendingRestoresRef.current.delete(character.path);
				}
			}
		}

		if (event.event === 'sprite_loaded') {
			const sprite = event as SpriteLoaded;
			dispatch({ type: 'ADD_SPRITE', payload: { path: sprite.path, name: sprite.name, width: sprite.width, height: sprite.height } });
		}

		if (event.event === 'sprite_removed') {
			const sprite = event as SpriteRemoved;
			dispatch({ type: 'REMOVE_SPRITE', payload: sprite.path });
		}

		if (event.event === 'sprites_list') {
			const spritesList = event as SpritesList;
			dispatch({ type: 'SET_SPRITES', payload: spritesList.sprites });
		}

		if (event.event === 'stopped') {
			dispatch({ type: 'ENGINE_STOPPED', payload: (event as { code?: number }).code });
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
			dispatch({ type: 'ADD_COLLIDER', payload: { id, path: '[Colisionador]' } });
			dispatch({ type: 'SET_TOOL_PROGRESS', payload: null });
		}

		if (event.event === 'tool_cancelled') {
			dispatch({ type: 'SET_TOOL_PROGRESS', payload: null });
		}

		if (event.event === 'pivot_selected') {
			const pivot = event as PivotSelected;
			refs.pivotEditListenerRef.current?.(pivot.frame_path, pivot.pivot_x, pivot.pivot_y);
		}

		if (event.event === 'animation_finished') {
			const animationFinished = event as AnimationFinished;
			const pending = refs.pendingEventsRef.current.get('animation_finished');
			if (pending) {
				pending.resolve(animationFinished);
				refs.pendingEventsRef.current.delete('animation_finished');
			}
			dispatch({ type: 'SET_ANIMATION_PLAYING', payload: { entityId: animationFinished.entity_id, playing: false } });
		}

		if (event.event === 'physics_changed') {
			const physicsChanged = event as PhysicsChanged;
			if (refs.entityMetaRef.current[physicsChanged.entity_id]) {
				refs.entityMetaRef.current[physicsChanged.entity_id].physicsEnabled = physicsChanged.enabled;
				refs.entityMetaRef.current[physicsChanged.entity_id].physicsType = physicsChanged.body_type;
			}
			dispatch({
				type: 'UPDATE_SELECTED_PHYSICS',
				payload: { entityId: physicsChanged.entity_id, enabled: physicsChanged.enabled, bodyType: physicsChanged.body_type },
			});
		}
	};
}