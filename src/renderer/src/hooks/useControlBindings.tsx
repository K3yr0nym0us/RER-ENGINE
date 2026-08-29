import { useCallback, useMemo, useState } from 'react'

import ScriptEditorModalBody from '../components/SpritePreviewModalBody/components/ScriptEditorModalBody'
import { ControlBindingsModalBody } from '../pages/EngineView/components/sidebar/ControlsAccordion/components/ControlBindingsModalBody'

import { useContextEngine } from '@engine'
import { useModal } from '@modal'
import type { SavedControlBindings } from '@shared-types'
import { isEditorCameraEntity, isEditorCameraPath, isPlayerPath } from '@shared-types'
import { useLanguage } from '../context/LanguageContext'
import { getDefaultControlScript } from '../editor/rhaiScriptTemplates'

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
  const {
    characterEntities,
    entityMetaRef,
    editorCameraEntityIdRef,
    playerEntityIdRef,
    playCharacterViewSyncSeq,
    projectType,
    send,
  } = useContextEngine()
  const { openModal } = useModal()
  const { locale } = useLanguage()

  const [selectedCharacterId, setSelectedCharacterId] = useState<number | null>(null)
  const [revision, setRevision] = useState(0)

  const characterOptions = useMemo<CharacterOption[]>(() => {
    void playCharacterViewSyncSeq
    const seen = new Set<number>()
    const options: CharacterOption[] = []
    for (const character of characterEntities) {
      if (seen.has(character.id)) continue
      if (isEditorCameraPath(character.path)) continue
      if (isEditorCameraEntity(character.id, entityMetaRef.current[character.id], editorCameraEntityIdRef.current)) {
        continue
      }
      seen.add(character.id)
      const name = entityMetaRef.current[character.id]?.name?.trim()
      options.push({
        id: character.id,
        label: name && name.length > 0 ? name : getPathLabel(character.path),
      })
    }
    const playerId = playerEntityIdRef.current
    if (playerId != null && !seen.has(playerId)) {
      const meta = entityMetaRef.current[playerId]
      const path = meta?.path ?? '[Player]'
      if (isPlayerPath(path)) {
        seen.add(playerId)
        const name = meta?.name?.trim()
        options.unshift({
          id: playerId,
          label: name && name.length > 0 ? name : getPathLabel('[Player]'),
        })
      }
    }
    return options
  }, [characterEntities, entityMetaRef, editorCameraEntityIdRef, playerEntityIdRef, playCharacterViewSyncSeq])

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
    void revision
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
    send({ cmd: 'set_control_bindings', id: effectiveCharacterId, bindings: meta.controlBindings } as never)
    setRevision((prev) => prev + 1)
  }, [effectiveCharacterId, entityMetaRef, send])

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
            initialData={existing ?? {
              name: `${mode}_${controlKey.toLowerCase()}`,
              source: getDefaultControlScript(locale, {
                controlKey,
                projectType: projectType === '3D' ? '3D' : '2D',
              }),
            }}
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
  }, [effectiveCharacterId, getBindingsForCurrentCharacter, locale, openModal, projectType, selectedCharacterLabel, setBinding])

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
