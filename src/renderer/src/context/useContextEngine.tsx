import React, { useReducer, useRef, useEffect, createContext, useContext } from 'react';
import type { EngineEvent, EntitySelected, ScenarioLoaded, CharacterLoaded, PlayerReady, Camera2dUpdated, ProjectSaveData, PivotSelected, AnimationFinished } from '../../../shared-types/types';

// Tipos y estado inicial (idénticos al hook original)
export interface Entity {
	id: number
}
export interface SelectedEntity {
	id: number
	name: string
	position: [number, number, number]
	rotation: [number, number, number, number]
	scale: [number, number, number]
	physicsEnabled: boolean
	physicsType: string
	path?: string
	animations?: {
		name:      string
		fps:       number
		loop:      boolean
		logical_w: number
		logical_h: number
		frames: {
			path:    string
			pivot_x: number
			pivot_y: number
		}[]
	}[]
	scripts?: { name: string; source: string }[]
}
export interface LogEntry {
	id: number
	text: string
	isError: boolean
}
export interface ScenarioEntry {
	id: number
	path: string
}
export type CharacterEntry = ScenarioEntry
export interface WorldConfig {
	worldWidth: number
	worldHeight: number
	gridVisible: boolean
	gridCellSize: number
}
const DEFAULT_WORLD_CONFIG: WorldConfig = {
	worldWidth: 100,
	worldHeight: 50,
	gridVisible: true,
	gridCellSize: 1,
};
interface EngineState {
	engineReady: boolean
	engineError: string | null
	log: LogEntry[]
	entities: Entity[]
	selectedEntity: SelectedEntity | null
	hoveredEntityId: number | null
	backgroundPath: string | null
	scenarioEntities: ScenarioEntry[]
	characterEntities: CharacterEntry[]
	worldConfig: WorldConfig
	colliderEntities: ScenarioEntry[]
	toolProgress: number | null
	animationPlaying: Map<number, boolean>
}
type EngineAction =
	| { type: 'SET_READY' }
	| { type: 'SET_ERROR'; payload: string }
	| { type: 'ADD_LOG'; payload: LogEntry }
	| { type: 'ADD_ENTITY'; payload: number }
	| { type: 'SELECT_ENTITY'; payload: SelectedEntity }
	| { type: 'DESELECT_ENTITY' }
	| { type: 'ENGINE_STOPPED'; payload: number | undefined }
	| { type: 'CLEAR_ENTITIES' }
	| { type: 'RESET_ENGINE' }
	| { type: 'ADD_SCENARIO'; payload: ScenarioEntry }
	| { type: 'REMOVE_SCENARIO'; payload: number }
	| { type: 'ADD_CHARACTER'; payload: CharacterEntry }
	| { type: 'REMOVE_CHARACTER'; payload: number }
	| { type: 'SET_HOVER'; payload: number | null }
	| { type: 'SET_BACKGROUND'; payload: string | null }
	| { type: 'SET_WORLD_CONFIG'; payload: Partial<WorldConfig> }
	| { type: 'ADD_COLLIDER'; payload: ScenarioEntry }
	| { type: 'REMOVE_COLLIDER'; payload: number }
	| { type: 'SET_TOOL_PROGRESS'; payload: number | null }
	| { type: 'SET_ANIMATION_PLAYING'; payload: { entityId: number; playing: boolean } }

const initialState: EngineState = {
	engineReady: false,
	engineError: null,
	log: [],
	entities: [],
	selectedEntity: null,
	hoveredEntityId: null,
	backgroundPath: null,
	scenarioEntities: [],
	characterEntities: [],
	worldConfig: DEFAULT_WORLD_CONFIG,
	colliderEntities: [],
	toolProgress: null,
	animationPlaying: new Map(),
}

function engineReducer(state: EngineState, action: EngineAction): EngineState {
	const handlers: Record<string, (state: EngineState, action: any) => EngineState> = {
		SET_READY: (state) => ({ ...state, engineReady: true, engineError: null }),
		SET_ERROR: (state, action) => ({ ...state, engineError: action.payload }),
		ADD_LOG: (state, action) => ({ ...state, log: [...state.log.slice(-199), action.payload] }),
		ADD_ENTITY: (state, action) =>
			state.entities.some((e) => e.id === action.payload)
				? state
				: { ...state, entities: [...state.entities, { id: action.payload }] },
		SELECT_ENTITY: (state, action) => ({ ...state, selectedEntity: action.payload }),
		DESELECT_ENTITY: (state) => ({ ...state, selectedEntity: null }),
		ENGINE_STOPPED: (state, action) => {
			const code = action.payload;
			const error = (code !== 0 && code != null)
				? `El motor terminó inesperadamente (código ${code}).`
				: null;
			return { ...state, engineReady: false, ...(error ? { engineError: error } : {}) };
		},
		CLEAR_ENTITIES: (state) => ({ ...state, entities: [] }),
		RESET_ENGINE: (state) => ({ ...state, engineReady: false, engineError: null, entities: [] }),
		ADD_SCENARIO: (state, action) => ({ ...state, scenarioEntities: [...state.scenarioEntities, action.payload] }),
		REMOVE_SCENARIO: (state, action) => ({
			...state,
			scenarioEntities: state.scenarioEntities.filter((s) => s.id !== action.payload)
		}),
		ADD_CHARACTER: (state, action) => ({ ...state, characterEntities: [...state.characterEntities, action.payload] }),
		REMOVE_CHARACTER: (state, action) => ({
			...state,
			characterEntities: state.characterEntities.filter((c) => c.id !== action.payload)
		}),
		SET_HOVER: (state, action) => ({ ...state, hoveredEntityId: action.payload }),
		SET_BACKGROUND: (state, action) => ({ ...state, backgroundPath: action.payload }),
		SET_WORLD_CONFIG: (state, action) => ({
			...state,
			worldConfig: { ...state.worldConfig, ...action.payload }
		}),
		ADD_COLLIDER: (state, action) => ({ ...state, colliderEntities: [...state.colliderEntities, action.payload] }),
		REMOVE_COLLIDER: (state, action) => ({
			...state,
			colliderEntities: state.colliderEntities.filter((c) => c.id !== action.payload)
		}),
		SET_TOOL_PROGRESS: (state, action) => ({ ...state, toolProgress: action.payload }),
		SET_ANIMATION_PLAYING: (state, action) => {
			const newMap = new Map(state.animationPlaying);
			newMap.set(action.payload.entityId, action.payload.playing);
			return { ...state, animationPlaying: newMap };
		},
	};
	const handler = handlers[action.type as keyof typeof handlers];
	return handler ? handler(state, action) : state;
}

// Contexto y provider
interface EngineContextValue extends EngineState {
	entityTransformsRef: React.MutableRefObject<Record<number, any>>;
	entityMetaRef: React.MutableRefObject<Record<number, any>>;
	playerEntityIdRef: React.MutableRefObject<number | null>;
	camera2dRef: React.MutableRefObject<any>;
	send: (cmd: object) => void;
	sendAsync: <T>(cmd: object, waitForEvent: string, onStart?: () => void) => Promise<T>;
	setAnimationPlaying: (entityId: number, playing: boolean) => void;
	loadModel: (path: string) => void;
	reportBounds: () => void;
	retryEngine: () => void;
	removeScenario: (id: number) => void;
	duplicateScenario: (id: number) => void;
	removeCharacter: (id: number) => void;
	duplicateCharacter: (id: number) => void;
	setWorldSize: (width: number, height: number) => void;
	setGridVisible: (visible: boolean) => void;
	setGridCellSize: (size: number) => void;
	removeCollider: (id: number) => void;
	updateEntityAnimations: (id: number, animations: any[]) => void;
	updateEntityScripts: (id: number, scripts: { name: string; source: string }[]) => void;
	registerPivotEditListener: (fn: (framePath: string, px: number, py: number) => void) => void;
	unregisterPivotEditListener: () => void;
}

const EngineContext = createContext<EngineContextValue | undefined>(undefined);

export function EngineProvider({
	children,
	viewportRef,
	projectType,
	initialSave,
}: {
	children: React.ReactNode;
	viewportRef: React.RefObject<HTMLDivElement | null>;
	projectType?: string;
	initialSave?: ProjectSaveData | null;
}) {
	// Copia la lógica de useEngine aquí, pero sin return, sino value del context
	const [state, dispatch] = useReducer(engineReducer, initialState);
	const readyTimer         = useRef<ReturnType<typeof setTimeout> | null>(null);
	const logIdRef           = useRef(0);
	const initialSaveRef     = useRef(initialSave);

	type Transform = { position: [number,number,number]; rotation: [number,number,number,number]; scale: [number,number,number] };
	const entityTransformsRef = useRef<Record<number, Transform>>({});
	type ColliderPoints = [[number,number],[number,number],[number,number],[number,number]];
	type EntityMeta = {
		kind: 'scenario' | 'character' | 'model' | 'collider';
		path: string;
		physicsEnabled: boolean;
		physicsType: string;
		points?: ColliderPoints;
		animations?: {
			name:      string
			fps:       number
			loop:      boolean
			logical_w: number
			logical_h: number
			frames: {
				path:    string
				pivot_x: number
				pivot_y: number
			}[]
		}[]
		scripts?: { name: string; source: string }[]
	};
	const entityMetaRef = useRef<Record<number, EntityMeta>>({});
	type PendingRestore = { transform: Transform; physicsEnabled: boolean; physicsType: string; animations?: any[]; scripts?: { name: string; source: string }[] };
	const pendingRestoresRef = useRef<Map<string, PendingRestore[]>>(new Map());
	const playerEntityIdRef = useRef<number | null>(null);
	type Camera2dState = { x: number; y: number; halfH: number };
	const camera2dRef = useRef<Camera2dState | null>(null);
	const mainPlayerHandled = useRef(false);
	const playerRemoved     = useRef(false);
	const pendingPlayerDups = useRef<Transform[]>([]);
	const pendingDupQ       = useRef<Transform[]>([]);
	// Callback registrado por AnimationsPanel para recibir pivot seleccionado
	const pivotEditListenerRef = useRef<((framePath: string, px: number, py: number) => void) | null>(null);
	// Pending promises para sendAsync
	const pendingEventsRef = useRef<Map<string, { resolve: (value: any) => void }>>(new Map());

	const addLog = (text: string, isError = false) => {
		logIdRef.current += 1;
		dispatch({ type: 'ADD_LOG', payload: { id: logIdRef.current, text, isError } });
	};

	const reportBounds = () => {
		if (!viewportRef.current) return;
		const rect = viewportRef.current.getBoundingClientRect();
		const dpr  = window.devicePixelRatio ?? 1;
		window.electronAPI.sendViewportBounds({
			x:      rect.x      * dpr,
			y:      rect.y      * dpr,
			width:  rect.width  * dpr,
			height: rect.height * dpr,
		});
	};

	const send = (cmd: object) => window.engine.send(cmd as never);

	const sendAsync = <T,>(cmd: object, waitForEvent: string, onStart?: () => void): Promise<T> => {
		if (onStart) onStart();
		return new Promise((resolve) => {
			pendingEventsRef.current.set(waitForEvent, { resolve });
			window.engine.send(cmd as never);
		});
	};

	const setAnimationPlaying = (entityId: number, playing: boolean) => {
		dispatch({ type: 'SET_ANIMATION_PLAYING', payload: { entityId, playing } });
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

	useEffect(() => {
		reportBounds();
		const observer = new ResizeObserver(reportBounds);
		if (viewportRef.current) observer.observe(viewportRef.current);
		window.electronAPI.onRequestViewportBounds(reportBounds);
		return () => observer.disconnect();
	}, []);

	useEffect(() => {
		const onKeyDown = (e: KeyboardEvent) => {
			if (e.key === 'Control') window.engine.send({ cmd: 'set_ctrl_held', held: true } as never);
		};
		const onKeyUp = (e: KeyboardEvent) => {
			if (e.key === 'Control') window.engine.send({ cmd: 'set_ctrl_held', held: false } as never);
		};
		window.addEventListener('keydown', onKeyDown);
		window.addEventListener('keyup',   onKeyUp);
		return () => {
			window.removeEventListener('keydown', onKeyDown);
			window.removeEventListener('keyup',   onKeyUp);
		};
	}, []);

	useEffect(() => {
		readyTimer.current = setTimeout(() => {
			dispatch({ type: 'SET_ERROR', payload: 'El motor no respondió en 5 segundos. Puede que el binario no exista o haya fallado al iniciar.' });
			addLog('[timeout] Motor no respondió en 5s', true);
		}, 5000);
		return () => {
			if (readyTimer.current) clearTimeout(readyTimer.current);
		};
	}, []);

	useEffect(() => {
		window.engine.on((event: EngineEvent) => {
			addLog(JSON.stringify(event), event.event === 'error')

			if (event.event === 'ready') {
				dispatch({ type: 'SET_READY' })
				if (readyTimer.current) clearTimeout(readyTimer.current)
				if (projectType) {
					window.engine.send({ cmd: 'set_scene', scene: projectType } as never)
				}
				mainPlayerHandled.current = false
				playerRemoved.current     = false
				pendingPlayerDups.current = []
				pendingDupQ.current       = []
				const save = initialSaveRef.current
				if (save) {
					if (save.world) {
						dispatch({ type: 'SET_WORLD_CONFIG', payload: save.world })
						window.engine.send({ cmd: 'set_world_size',    width:   save.world.worldWidth,   height: save.world.worldHeight } as never)
						window.engine.send({ cmd: 'set_grid_visible',  visible: save.world.gridVisible } as never)
						window.engine.send({ cmd: 'set_grid_cell_size', size:   save.world.gridCellSize } as never)
					}
					if (save.camera2d) {
						window.engine.send({ cmd: 'set_camera2d', x: save.camera2d.x, y: save.camera2d.y, half_h: save.camera2d.halfH } as never)
						camera2dRef.current = save.camera2d
					}
					if (save.backgroundPath) {
						window.engine.send({ cmd: 'load_background', path: save.backgroundPath } as never)
					}
					for (const entity of save.entities) {
						const t: Transform = {
							position: entity.position,
							rotation: entity.rotation,
							scale:    entity.scale,
						}
						if (entity.kind === 'collider' && entity.points) {
							window.engine.send({ cmd: 'create_collider_from_points', points: entity.points } as never)
						} else if (entity.kind === 'character' && entity.path === '[Player]') {
							pendingPlayerDups.current.push(t)
						} else {
							const pr: PendingRestore = {
								transform:      t,
								physicsEnabled: entity.physics_enabled ?? false,
								physicsType:    entity.physics_type    ?? 'static',
								animations:    entity.animations,							scripts:       entity.scripts,							}
							const queue = pendingRestoresRef.current.get(entity.path) ?? []
							queue.push(pr)
							pendingRestoresRef.current.set(entity.path, queue)
							// Las animaciones ya están en pendingRestores; se enviarán al motor
							// en scenario_loaded/character_loaded con el ID real que asigne el motor.
							if (entity.kind === 'scenario')  window.engine.send({ cmd: 'load_scenario',  path: entity.path } as never)
							if (entity.kind === 'character') window.engine.send({ cmd: 'load_character', path: entity.path } as never)
							if (entity.kind === 'model')     window.engine.send({ cmd: 'load_model',     path: entity.path } as never)
						}
					}
				}
			}
			if (event.event === 'model_loaded') {
				const id = (event as { id?: number }).id ?? -1
				dispatch({ type: 'ADD_ENTITY', payload: id })
			}
			if (event.event === 'entity_selected') {
				const e = event as unknown as EntitySelected
				entityTransformsRef.current[e.id] = { position: e.position, rotation: e.rotation, scale: e.scale }
				if (entityMetaRef.current[e.id]) {
					entityMetaRef.current[e.id].physicsEnabled = e.physics_enabled ?? false
					entityMetaRef.current[e.id].physicsType    = e.physics_type    ?? ''
				}
				const meta = entityMetaRef.current[e.id]
				dispatch({ type: 'SELECT_ENTITY', payload: {
					id:             e.id,
					name:           e.name,
					position:       e.position,
					rotation:       e.rotation,
					scale:          e.scale,
					physicsEnabled: e.physics_enabled ?? false,
					physicsType:    e.physics_type    ?? '',
					path:           meta?.path,
					animations:     meta?.animations,
					scripts:        meta?.scripts,
				} })
			}
			if (event.event === 'entity_deselected') {
				dispatch({ type: 'DESELECT_ENTITY' })
			}
			if (event.event === 'entity_hovered') {
				dispatch({ type: 'SET_HOVER', payload: (event as { id?: number }).id ?? null })
			}
			if (event.event === 'entity_unhovered') {
				dispatch({ type: 'SET_HOVER', payload: null })
			}
			if (event.event === 'player_ready') {
				const e = event as unknown as PlayerReady
				playerEntityIdRef.current = e.id
				entityTransformsRef.current[e.id] = {
					position: e.position,
					rotation: [0, 0, 0, 1],
					scale:    e.scale,
				}
				entityMetaRef.current[e.id] = { kind: 'character', path: '[Player]', physicsEnabled: false, physicsType: '' }
				const save = initialSaveRef.current
				if (save != null && save.playerTransform === null) {
					window.engine.send({ cmd: 'remove_entity', id: e.id } as never)
					playerEntityIdRef.current = null
					playerRemoved.current     = true
					delete entityMetaRef.current[e.id]
				} else if (save?.playerTransform) {
					window.engine.send({
						cmd:      'set_transform',
						id:       e.id,
						position: save.playerTransform.position,
						scale:    save.playerTransform.scale,
					} as never)
					entityTransformsRef.current[e.id] = {
						position: save.playerTransform.position,
						rotation: [0, 0, 0, 1],
						scale:    save.playerTransform.scale,
					}
					for (const dupT of pendingPlayerDups.current) {
						pendingDupQ.current.push(dupT)
						window.engine.send({ cmd: 'duplicate_character', id: e.id } as never)
					}
					pendingPlayerDups.current = []
				}
			}
			if (event.event === 'camera_2d_updated') {
				const e = event as unknown as Camera2dUpdated
				camera2dRef.current = { x: e.x, y: e.y, halfH: e.half_h }
			}
			if (event.event === 'background_loaded') {
				dispatch({ type: 'SET_BACKGROUND', payload: (event as { path?: string }).path ?? null })
			}
			if (event.event === 'scenario_loaded') {
				const e = event as unknown as ScenarioLoaded
				dispatch({ type: 'ADD_SCENARIO', payload: { id: e.id, path: e.path } })
				entityMetaRef.current[e.id] = { kind: 'scenario', path: e.path, physicsEnabled: false, physicsType: '' }
				const queue = pendingRestoresRef.current.get(e.path)
				if (queue && queue.length > 0) {
					const pending = queue.shift()!
					window.engine.send({ cmd: 'set_transform', id: e.id, position: pending.transform.position, rotation: pending.transform.rotation, scale: pending.transform.scale } as never)
					entityTransformsRef.current[e.id] = pending.transform
					if (pending.physicsEnabled) {
						window.engine.send({ cmd: 'set_physics', id: e.id, enabled: true, body_type: pending.physicsType } as never)
						entityMetaRef.current[e.id].physicsEnabled = true
						entityMetaRef.current[e.id].physicsType    = pending.physicsType
					}
					if (pending.animations) {
						entityMetaRef.current[e.id].animations = pending.animations
						// Sincronizar animaciones con el motor usando el ID real asignado por el motor.
						for (const anim of pending.animations) {
							window.engine.send({
								cmd:        'set_animation',
								id:         e.id,
								name:       anim.name,
								frames:     anim.frames,
								fps:        anim.fps,
								loop_:      anim.loop,
								audio_path: anim.audio_path ?? null,
								logical_w:  anim.logical_w ?? 64,
								logical_h:  anim.logical_h ?? 64,
							} as never)
						}
					}
					if (pending.scripts) {
						entityMetaRef.current[e.id].scripts = pending.scripts
						for (const s of pending.scripts) {
							window.engine.send({ cmd: 'load_script', id: e.id, path: s.name, source: s.source } as never)
						}
					}
					if (queue.length === 0) pendingRestoresRef.current.delete(e.path)
				}
			}
			if (event.event === 'character_loaded') {
				const e = event as unknown as CharacterLoaded
				if (e.path === '[Player]') {
					if (!mainPlayerHandled.current) {
						mainPlayerHandled.current = true
						if (!playerRemoved.current) {
							dispatch({ type: 'ADD_CHARACTER', payload: { id: e.id, path: e.path } })
						}
						playerRemoved.current = false
					} else {
						dispatch({ type: 'ADD_CHARACTER', payload: { id: e.id, path: e.path } })
						entityMetaRef.current[e.id] = { kind: 'character', path: '[Player]', physicsEnabled: false, physicsType: '' }
						const dupT = pendingDupQ.current.shift()
						if (dupT) {
							window.engine.send({ cmd: 'set_transform', id: e.id, position: dupT.position, rotation: dupT.rotation, scale: dupT.scale } as never)
							entityTransformsRef.current[e.id] = dupT
						}
					}
				} else {
					dispatch({ type: 'ADD_CHARACTER', payload: { id: e.id, path: e.path } })
					const existingMeta = entityMetaRef.current[e.id]
					if (existingMeta) {
						entityMetaRef.current[e.id] = { ...existingMeta }
					} else {
						entityMetaRef.current[e.id] = { kind: 'character', path: e.path, physicsEnabled: false, physicsType: '' }
					}
					const queue = pendingRestoresRef.current.get(e.path)
					if (queue && queue.length > 0) {
						const pending = queue.shift()!
						window.engine.send({ cmd: 'set_transform', id: e.id, position: pending.transform.position, rotation: pending.transform.rotation, scale: pending.transform.scale } as never)
						entityTransformsRef.current[e.id] = pending.transform
						if (pending.physicsEnabled) {
							window.engine.send({ cmd: 'set_physics', id: e.id, enabled: true, body_type: pending.physicsType } as never)
							entityMetaRef.current[e.id].physicsEnabled = true
							entityMetaRef.current[e.id].physicsType    = pending.physicsType
						}
						if (pending.animations) {
							entityMetaRef.current[e.id].animations = pending.animations
							// Sincronizar animaciones con el motor usando el ID real asignado por el motor.
							for (const anim of pending.animations) {
								window.engine.send({
									cmd:        'set_animation',
									id:         e.id,
									name:       anim.name,
									frames:     anim.frames,
									fps:        anim.fps,
									loop_:      anim.loop,
									audio_path: anim.audio_path ?? null,
									logical_w:  anim.logical_w ?? 64,
									logical_h:  anim.logical_h ?? 64,
								} as never)
							}
						}
						if (pending.scripts) {
							entityMetaRef.current[e.id].scripts = pending.scripts
							for (const s of pending.scripts) {
								window.engine.send({ cmd: 'load_script', id: e.id, path: s.name, source: s.source } as never)
							}
						}
						if (queue.length === 0) pendingRestoresRef.current.delete(e.path)
					}
				}
			}
			if (event.event === 'stopped') {
				dispatch({ type: 'ENGINE_STOPPED', payload: (event as { code?: number }).code })
			}
			if (event.event === 'error') {
				dispatch({ type: 'SET_ERROR', payload: (event as { message?: string }).message ?? 'Error desconocido' })
			}
			if (event.event === 'drawing_progress') {
				dispatch({ type: 'SET_TOOL_PROGRESS', payload: (event as { count?: number }).count ?? 0 })
			}
			if (event.event === 'collider_created') {
				const ev = event as { id?: number; points?: [[number,number],[number,number],[number,number],[number,number]] }
				const id  = ev.id ?? -1
				entityMetaRef.current[id] = { kind: 'collider', path: '[Colisionador]', physicsEnabled: true, physicsType: 'static', points: ev.points }
				dispatch({ type: 'ADD_COLLIDER', payload: { id, path: '[Colisionador]' } })
				dispatch({ type: 'SET_TOOL_PROGRESS', payload: null })
			}
			if (event.event === 'tool_cancelled') {
				dispatch({ type: 'SET_TOOL_PROGRESS', payload: null })
			}
			if (event.event === 'pivot_selected') {
				const e = event as unknown as PivotSelected;
				pivotEditListenerRef.current?.(e.frame_path, e.pivot_x, e.pivot_y);
			}
			if (event.event === 'animation_finished') {
				const e = event as unknown as AnimationFinished;
				const pending = pendingEventsRef.current.get('animation_finished');
				if (pending) {
					pending.resolve(e);
					pendingEventsRef.current.delete('animation_finished');
				}
				dispatch({ type: 'SET_ANIMATION_PLAYING', payload: { entityId: e.entity_id, playing: false } });
			}
		})
		return () => { window.engine.off() }
	}, [])

	// Acciones igual que en el hook
	const removeScenario = (id: number) => {
		send({ cmd: 'remove_entity', id });
		dispatch({ type: 'REMOVE_SCENARIO', payload: id });
		delete entityMetaRef.current[id];
	};
	const duplicateScenario = (id: number) => {
		send({ cmd: 'duplicate_scenario', id });
	};
	const removeCharacter = (id: number) => {
		send({ cmd: 'remove_entity', id });
		dispatch({ type: 'REMOVE_CHARACTER', payload: id });
		if (playerEntityIdRef.current === id) playerEntityIdRef.current = null;
		delete entityMetaRef.current[id];
	};
	const duplicateCharacter = (id: number) => {
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
	const removeCollider = (id: number) => {
		send({ cmd: 'remove_entity', id });
		dispatch({ type: 'REMOVE_COLLIDER', payload: id });
		delete entityMetaRef.current[id];
	};
	const updateEntityAnimations = (id: number, animations: any[]) => {
		if (!entityMetaRef.current[id]) {
			entityMetaRef.current[id] = { kind: 'model', path: '', physicsEnabled: false, physicsType: '' }
		}
		entityMetaRef.current[id].animations = animations
		// Sincronizar cada animación con el motor para que `play_animation` no
		// necesite reenviar los datos en cada reproducción.
		for (const anim of animations) {
			window.engine.send({
				cmd:       'set_animation',
				id,
				name:      anim.name,
				frames:    anim.frames,
				fps:       anim.fps,
				loop_:     anim.loop,
				audio_path: anim.audio_path ?? null,
				logical_w: anim.logical_w ?? 64,
				logical_h: anim.logical_h ?? 64,
			} as never)
		}
	};
	const updateEntityScripts = (id: number, scripts: { name: string; source: string }[]) => {
		if (!entityMetaRef.current[id]) {
			entityMetaRef.current[id] = { kind: 'model', path: '', physicsEnabled: false, physicsType: '' }
		}
		entityMetaRef.current[id].scripts = scripts
	};
	const registerPivotEditListener = (fn: (framePath: string, px: number, py: number) => void) => {
		pivotEditListenerRef.current = fn;
	};
	const unregisterPivotEditListener = () => {
		pivotEditListenerRef.current = null;
	};

	const value: EngineContextValue = {
		...state,
		entityTransformsRef,
		entityMetaRef,
		playerEntityIdRef,
		camera2dRef,
		send,
		sendAsync,
		setAnimationPlaying,
		loadModel,
		reportBounds,
		retryEngine,
		removeScenario,
		duplicateScenario,
		removeCharacter,
		duplicateCharacter,
		setWorldSize,
		setGridVisible,
		setGridCellSize,
		removeCollider,
		updateEntityAnimations,
		updateEntityScripts,
		registerPivotEditListener,
		unregisterPivotEditListener,
	};

	return (
		<EngineContext.Provider value={value}>
			{children}
		</EngineContext.Provider>
	);
}

export function useContextEngine() {
	const ctx = useContext(EngineContext);
	if (!ctx) throw new Error('useContextEngine debe usarse dentro de <EngineProvider>');
	return ctx;
}
