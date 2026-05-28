import { useEffect, useRef, type Dispatch, type RefObject } from 'react';
import type { GameStyle } from '@shared-types';
import type { Locale } from '../LanguageContext';
import { createEngineEventHandler } from './createEngineEventHandler';
import {
	beginEngineBootEntityWait,
	beginEngineBootLoading,
	isEngineBootScenePreloaded,
} from './sceneImportOverlay';
import type { EngineAction, EngineInternalRefs } from '../types';

const ENGINE_READY_TIMEOUT_MS_2D = 5_000;
const ENGINE_READY_TIMEOUT_MS_3D = 30_000;

interface UseEngineEffectsParams {
	dispatch: Dispatch<EngineAction>
	refs: EngineInternalRefs
	addLog: (text: string, isError?: boolean) => void
	viewportRef: RefObject<HTMLDivElement | null>
	projectType?: string
	gameStyle?: GameStyle
	reportBounds: () => void
	reportBoundsDebounced: () => void
	applyInitialAnimationFrame: (entityId: number, animations?: any[]) => void
	setLocale?: (locale: Locale) => void
}

export function useEngineEffects({
	dispatch,
	refs,
	addLog,
	viewportRef,
	projectType,
	gameStyle,
	reportBounds,
	reportBoundsDebounced,
	applyInitialAnimationFrame,
	setLocale,
}: UseEngineEffectsParams) {
	const reportBoundsRef = useRef(reportBounds);
	const reportBoundsDebouncedRef = useRef(reportBoundsDebounced);
	const addLogRef = useRef(addLog);
	const projectTypeRef = useRef(projectType);
	const gameStyleRef = useRef(gameStyle);
	const applyInitialAnimationFrameRef = useRef(applyInitialAnimationFrame);
	const engineEventHandlerRef = useRef(createEngineEventHandler({
		dispatch,
		refs,
		addLog,
		projectType,
		gameStyle,
		applyInitialAnimationFrame,
		setLocale,
		reportBounds,
	}));

	useEffect(() => {
		reportBoundsRef.current = reportBounds;
		reportBoundsDebouncedRef.current = reportBoundsDebounced;
		addLogRef.current = addLog;
		projectTypeRef.current = projectType;
		gameStyleRef.current = gameStyle;
		applyInitialAnimationFrameRef.current = applyInitialAnimationFrame;
		engineEventHandlerRef.current = createEngineEventHandler({
			dispatch,
			refs,
			addLog,
			projectType,
			gameStyle,
			applyInitialAnimationFrame,
			setLocale,
			reportBounds,
		});
	}, [dispatch, refs, addLog, projectType, gameStyle, applyInitialAnimationFrame, reportBounds, reportBoundsDebounced, setLocale]);

	useEffect(() => {
		const onRequestViewportBounds = () => reportBoundsRef.current();
		const onViewportResize = () => reportBoundsDebouncedRef.current();

		beginEngineBootLoading(dispatch);
		if (isEngineBootScenePreloaded(projectType, Boolean(refs.initialSaveRef.current))) {
			beginEngineBootEntityWait(refs);
		}
		reportBoundsRef.current();
		const observer = new ResizeObserver(onViewportResize);
		if (viewportRef.current) observer.observe(viewportRef.current);
		window.electronAPI.onRequestViewportBounds(onRequestViewportBounds);
		return () => {
			observer.disconnect();
			if (refs.resizeTimerRef.current) clearTimeout(refs.resizeTimerRef.current);
		};
	}, []);

	useEffect(() => {
		const isTypingTarget = (target: EventTarget | null) => {
			if (!(target instanceof HTMLElement)) return false;
			const tag = target.tagName.toLowerCase();
			return tag === 'input' || tag === 'textarea' || target.isContentEditable;
		};

		const onKeyDown = (event: KeyboardEvent) => {
			if (event.ctrlKey || event.key === 'Control') {
				window.engine.send({ cmd: 'set_ctrl_held', held: true } as never);
			}
			if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'z' && !isTypingTarget(event.target)) {
				event.preventDefault();
				if (event.shiftKey) {
					window.engine.send({ cmd: 'redo' } as never);
				} else {
					window.engine.send({ cmd: 'undo' } as never);
				}
			}
			if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'y' && !isTypingTarget(event.target)) {
				event.preventDefault();
				window.engine.send({ cmd: 'redo' } as never);
			}
		};
		const onKeyUp = (event: KeyboardEvent) => {
			if (event.key === 'Control' || !event.ctrlKey) {
				window.engine.send({ cmd: 'set_ctrl_held', held: false } as never);
			}
		};
		const onBlur = () => {
			window.engine.send({ cmd: 'set_ctrl_held', held: false } as never);
		};
		window.addEventListener('keydown', onKeyDown);
		window.addEventListener('keyup', onKeyUp);
		window.addEventListener('blur', onBlur);
		return () => {
			window.removeEventListener('keydown', onKeyDown);
			window.removeEventListener('keyup', onKeyUp);
			window.removeEventListener('blur', onBlur);
		};
	}, [dispatch]);

	useEffect(() => {
		const timeoutMs = projectType === '3D'
			? ENGINE_READY_TIMEOUT_MS_3D
			: ENGINE_READY_TIMEOUT_MS_2D;
		refs.readyTimer.current = setTimeout(() => {
			const seconds = Math.round(timeoutMs / 1000);
			dispatch({
				type: 'SET_ERROR',
				payload: `El motor no respondió en ${seconds} segundos. Puede que el binario no exista o haya fallado al iniciar.`,
			});
			addLogRef.current(`[timeout] Motor no respondió en ${seconds}s`, true);
		}, timeoutMs);
		return () => {
			if (refs.readyTimer.current) clearTimeout(refs.readyTimer.current);
		};
	}, [dispatch, projectType]);

	useEffect(() => {
		const handleEngineEvent = (event: { event: string; [key: string]: unknown }) => {
			engineEventHandlerRef.current(event);
		};

		window.engine.on(handleEngineEvent);
		return () => {
			window.engine.off(handleEngineEvent);
		};
	}, []);
}