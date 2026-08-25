import { useState, useEffect, useCallback, useRef } from 'react'
import type { EngineCommand2D, EngineCommand3D } from '@shared-types'

/**
 * Modo dibujo de objeto HUD: puntos indefinidos; el motor cierra al clicar el primer punto.
 */
export function usePlayerUiObjectDrawing(
	send: (cmd: EngineCommand2D | EngineCommand3D) => void,
	drawEndTick: number,
) {
	const [isActive, setIsActive] = useState(false)
	const isActiveRef = useRef(false)
	const drawEndTickAtStartRef = useRef(drawEndTick)

	useEffect(() => {
		if (!isActive) return
		if (drawEndTick !== drawEndTickAtStartRef.current) {
			isActiveRef.current = false
			setIsActive(false)
		}
	}, [drawEndTick, isActive])

	const start = useCallback(() => {
		drawEndTickAtStartRef.current = drawEndTick
		isActiveRef.current = true
		setIsActive(true)
		send({ cmd: 'set_player_ui_object_draw', active: true })
	}, [send, drawEndTick])

	const cancel = useCallback(() => {
		isActiveRef.current = false
		setIsActive(false)
		send({ cmd: 'set_player_ui_object_draw', active: false })
	}, [send])

	return { isActive, start, cancel, isActiveRef }
}
