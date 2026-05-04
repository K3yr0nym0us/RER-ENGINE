import { useEffect, type Dispatch, type RefObject } from 'react';
import { createEngineEventHandler } from './createEngineEventHandler';
import type { EngineAction, EngineInternalRefs } from '../types';

interface UseEngineEffectsParams {
	dispatch: Dispatch<EngineAction>
	refs: EngineInternalRefs
	addLog: (text: string, isError?: boolean) => void
	viewportRef: RefObject<HTMLDivElement | null>
	projectType?: string
	reportBounds: () => void
	reportBoundsDebounced: () => void
	applyInitialAnimationFrame: (entityId: number, animations?: any[]) => void
}

export function useEngineEffects({
	dispatch,
	refs,
	addLog,
	viewportRef,
	projectType,
	reportBounds,
	reportBoundsDebounced,
	applyInitialAnimationFrame,
}: UseEngineEffectsParams) {
	useEffect(() => {
		reportBounds();
		const observer = new ResizeObserver(reportBoundsDebounced);
		if (viewportRef.current) observer.observe(viewportRef.current);
		window.electronAPI.onRequestViewportBounds(reportBounds);
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
	}, []);

	useEffect(() => {
		refs.readyTimer.current = setTimeout(() => {
			dispatch({ type: 'SET_ERROR', payload: 'El motor no respondió en 5 segundos. Puede que el binario no exista o haya fallado al iniciar.' });
			addLog('[timeout] Motor no respondió en 5s', true);
		}, 5000);
		return () => {
			if (refs.readyTimer.current) clearTimeout(refs.readyTimer.current);
		};
	}, []);

	useEffect(() => {
		const handleEngineEvent = createEngineEventHandler({
			dispatch,
			refs,
			addLog,
			projectType,
			applyInitialAnimationFrame,
		});

		window.engine.on(handleEngineEvent);
		return () => {
			window.engine.off();
		};
	}, []);
}