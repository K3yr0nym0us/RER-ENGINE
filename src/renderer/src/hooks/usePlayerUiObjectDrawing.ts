import { useState, useEffect, useCallback, useRef } from 'react'

/**
 * Modo dibujo de objeto HUD: puntos indefinidos; el motor cierra al clicar el primer punto.
 */
export function usePlayerUiObjectDrawing(
	send: (cmd: object) => void,
	toolProgress: number | null,
) {
	const [isActive, setIsActive] = useState(false)
	const [pointCount, setPointCount] = useState(0)
	const sawEngineProgressRef = useRef(false)

	useEffect(() => {
		if (!isActive) return
		if (toolProgress === null) {
			if (sawEngineProgressRef.current) {
				setIsActive(false)
				setPointCount(0)
				sawEngineProgressRef.current = false
			}
		} else {
			sawEngineProgressRef.current = true
			setPointCount(toolProgress)
		}
	}, [toolProgress, isActive])

	const start = useCallback(() => {
		setIsActive(true)
		setPointCount(0)
		sawEngineProgressRef.current = false
		send({ cmd: 'set_player_ui_object_draw', active: true })
	}, [send])

	const cancel = useCallback(() => {
		setIsActive(false)
		setPointCount(0)
		sawEngineProgressRef.current = false
		send({ cmd: 'set_player_ui_object_draw', active: false })
	}, [send])

	return { isActive, pointCount, start, cancel }
}
