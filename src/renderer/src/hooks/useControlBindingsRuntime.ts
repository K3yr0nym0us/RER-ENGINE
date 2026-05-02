import { useEffect, useRef } from 'react'

import { useContextEngine } from '@engine'

const KEY_MAP: Record<string, string> = {
  KeyW: 'W',
  KeyA: 'A',
  KeyS: 'S',
  KeyD: 'D',
  Digit1: '1',
  Digit2: '2',
  Digit3: '3',
  Digit4: '4',
  Digit5: '5',
  Digit6: '6',
  Digit7: '7',
  Digit8: '8',
  Digit9: '9',
  Digit0: '0',
  Space: 'SPACE',
  ControlLeft: 'CTRL',
  ControlRight: 'CTRL',
  ShiftLeft: 'SHIFT',
  ShiftRight: 'SHIFT',
  AltLeft: 'ALT',
  AltRight: 'ALT',
}

const MOUSE_MAP: Record<number, string> = {
  0: 'MOUSE_LEFT',
  1: 'MOUSE_MIDDLE',
  2: 'MOUSE_RIGHT',
}

const GAMEPAD_MAP: Record<number, string> = {
  0: 'A',
  1: 'B',
  2: 'X',
  3: 'Y',
  4: 'LB',
  5: 'RB',
  6: 'LT',
  7: 'RT',
  8: 'BACK',
  9: 'START',
  10: 'L3',
  11: 'R3',
  12: 'D-UP',
  13: 'D-DOWN',
  14: 'D-LEFT',
  15: 'D-RIGHT',
}

export function useControlBindingsRuntime() {
  const { engineReady, previewPlaying, characterEntities, entityMetaRef, send } = useContextEngine()

  const pressedKeysRef = useRef<Set<string>>(new Set())
  const pressedMouseRef = useRef<Set<string>>(new Set())

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const mapped = KEY_MAP[event.code]
      if (!mapped) return
      pressedKeysRef.current.add(mapped)
    }

    const onKeyUp = (event: KeyboardEvent) => {
      const mapped = KEY_MAP[event.code]
      if (!mapped) return
      pressedKeysRef.current.delete(mapped)
    }

    const onMouseDown = (event: MouseEvent) => {
      const mapped = MOUSE_MAP[event.button]
      if (!mapped) return
      pressedMouseRef.current.add(mapped)
    }

    const onMouseUp = (event: MouseEvent) => {
      const mapped = MOUSE_MAP[event.button]
      if (!mapped) return
      pressedMouseRef.current.delete(mapped)
    }

    const onBlur = () => {
      pressedKeysRef.current.clear()
      pressedMouseRef.current.clear()
    }

    window.addEventListener('keydown', onKeyDown)
    window.addEventListener('keyup', onKeyUp)
    window.addEventListener('mousedown', onMouseDown)
    window.addEventListener('mouseup', onMouseUp)
    window.addEventListener('blur', onBlur)

    return () => {
      window.removeEventListener('keydown', onKeyDown)
      window.removeEventListener('keyup', onKeyUp)
      window.removeEventListener('mousedown', onMouseDown)
      window.removeEventListener('mouseup', onMouseUp)
      window.removeEventListener('blur', onBlur)
    }
  }, [])

  useEffect(() => {
    let rafId = 0

    const frame = () => {
      if (engineReady && previewPlaying) {
        const pressedKeyboardMouse = new Set<string>([
          ...pressedKeysRef.current,
          ...pressedMouseRef.current,
        ])

        const pressedGamepad = new Set<string>()
        const pads = navigator.getGamepads?.() ?? []
        for (const pad of pads) {
          if (!pad) continue
          pad.buttons.forEach((button, index) => {
            if (!button.pressed) return
            const mapped = GAMEPAD_MAP[index]
            if (mapped) pressedGamepad.add(mapped)
          })
        }

        if (pressedKeyboardMouse.size > 0 || pressedGamepad.size > 0) {
          for (const character of characterEntities) {
            const meta = entityMetaRef.current[character.id]
            if (!meta?.controlBindings) continue

            const keyboardBindings = meta.controlBindings.keyboard_mouse ?? {}
            const gamepadBindings = meta.controlBindings.gamepad ?? {}

            for (const controlKey of pressedKeyboardMouse) {
              const script = keyboardBindings[controlKey]
              if (!script) continue
              send({
                cmd: 'run_control_script',
                id: character.id,
                control_key: controlKey,
                path: script.name,
                source: script.source,
              })
            }

            for (const controlKey of pressedGamepad) {
              const script = gamepadBindings[controlKey]
              if (!script) continue
              send({
                cmd: 'run_control_script',
                id: character.id,
                control_key: controlKey,
                path: script.name,
                source: script.source,
              })
            }
          }
        }
      }

      rafId = window.requestAnimationFrame(frame)
    }

    rafId = window.requestAnimationFrame(frame)
    return () => window.cancelAnimationFrame(rafId)
  }, [characterEntities, engineReady, entityMetaRef, previewPlaying, send])
}

export default useControlBindingsRuntime
