import { useCallback, useMemo, useState } from 'react'

import ScriptEditorModalBody from '../pages/EngineView/components/ScriptEditorModalBody'
import { ControlBindingsModalBody } from '../pages/EngineView/components/sidebar/ControlsAccordion/ControlBindingsModalBody'

import { useContextEngine } from '@engine'
import { useModal } from '@modal'
import type { SavedControlBindings } from '@shared-types'

export type ControlDeviceMode = 'keyboard_mouse' | 'gamepad'

export interface ControlScript {
  name: string
  source: string
}

type ControlBindingsMap = Record<string, ControlScript>

interface CharacterBindings {
  keyboardMouse: ControlBindingsMap
  gamepad: ControlBindingsMap
}

interface CharacterOption {
  id: number
  label: string
}

const EMPTY_BINDINGS: CharacterBindings = {
  keyboardMouse: {},
  gamepad: {},
}

function fromSavedBindings(bindings?: SavedControlBindings): CharacterBindings {
  if (!bindings) return EMPTY_BINDINGS
  return {
    keyboardMouse: bindings.keyboard_mouse ?? {},
    gamepad: bindings.gamepad ?? {},
  }
}

function toSavedBindings(bindings: CharacterBindings): SavedControlBindings {
  return {
    keyboard_mouse: bindings.keyboardMouse,
    gamepad: bindings.gamepad,
  }
}

function getPathLabel(path: string): string {
  if (path === '[Player]') return 'Player principal'
  const normalized = path.split(/[/\\]/).pop() ?? path
  return normalized.replace(/\.[^/.]+$/, '')
}

export function useControlBindings() {
  const { characterEntities, entityMetaRef } = useContextEngine()
  const { openModal } = useModal()

  const [selectedCharacterId, setSelectedCharacterId] = useState<number | null>(null)
  const [revision, setRevision] = useState(0)

  const characterOptions = useMemo<CharacterOption[]>(() => {
    return characterEntities.map((character) => {
      const name = entityMetaRef.current[character.id]?.name?.trim()
      return {
        id: character.id,
        label: name && name.length > 0 ? name : getPathLabel(character.path),
      }
    })
  }, [characterEntities, entityMetaRef])

  const effectiveCharacterId = useMemo(() => {
    if (selectedCharacterId && characterOptions.some((item) => item.id === selectedCharacterId)) {
      return selectedCharacterId
    }
    return characterOptions[0]?.id ?? null
  }, [selectedCharacterId, characterOptions])

  const selectedCharacterLabel = useMemo(() => {
    const option = characterOptions.find((item) => item.id === effectiveCharacterId)
    return option?.label ?? 'Sin personaje'
  }, [characterOptions, effectiveCharacterId])

  const getBindingsForCurrentCharacter = useCallback((): CharacterBindings => {
    if (!effectiveCharacterId) return EMPTY_BINDINGS
    return fromSavedBindings(entityMetaRef.current[effectiveCharacterId]?.controlBindings)
  }, [effectiveCharacterId, entityMetaRef, revision])

  const setBinding = useCallback((mode: ControlDeviceMode, controlKey: string, script: ControlScript) => {
    if (!effectiveCharacterId) return
    const meta = entityMetaRef.current[effectiveCharacterId]
    if (!meta) return

    const current = fromSavedBindings(meta.controlBindings)
    const next: CharacterBindings = {
      keyboardMouse: mode === 'keyboard_mouse'
        ? { ...current.keyboardMouse, [controlKey]: script }
        : current.keyboardMouse,
      gamepad: mode === 'gamepad'
        ? { ...current.gamepad, [controlKey]: script }
        : current.gamepad,
    }

    meta.controlBindings = toSavedBindings(next)
    setRevision((prev) => prev + 1)
  }, [effectiveCharacterId, entityMetaRef])

  const openBindingsModal = useCallback((mode: ControlDeviceMode) => {
    if (!effectiveCharacterId) return

    const current = getBindingsForCurrentCharacter()
    const currentBindings = mode === 'keyboard_mouse' ? current.keyboardMouse : current.gamepad

    const openScriptEditor = (controlKey: string) => {
      const existing = currentBindings[controlKey]

      openModal({
        title: `Script para ${controlKey} (${selectedCharacterLabel})`,
        size: 'lg',
        body: (
          <ScriptEditorModalBody
            initialData={existing ?? { name: `${mode}_${controlKey.toLowerCase()}` }}
            onSave={(data) => {
              setBinding(mode, controlKey, data)
              openBindingsModal(mode)
            }}
            onCancel={() => openBindingsModal(mode)}
          />
        ),
      })
    }

    openModal({
      title: mode === 'keyboard_mouse' ? 'Controles: Teclado + Mouse' : 'Controles: Mandos',
      size: 'xl',
      body: (
        <ControlBindingsModalBody
          mode={mode}
          characterLabel={selectedCharacterLabel}
          bindings={currentBindings}
          onOpenScriptEditor={openScriptEditor}
        />
      ),
    })
  }, [effectiveCharacterId, getBindingsForCurrentCharacter, openModal, selectedCharacterLabel, setBinding])

  const currentBindings = getBindingsForCurrentCharacter()

  return {
    selectedCharacterId: effectiveCharacterId,
    setSelectedCharacterId,
    characterOptions,
    keyboardBindingsCount: Object.keys(currentBindings.keyboardMouse).length,
    gamepadBindingsCount: Object.keys(currentBindings.gamepad).length,
    openBindingsModal,
  }
}

export default useControlBindings
