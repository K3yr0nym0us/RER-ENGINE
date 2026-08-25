import { describe, expect, it } from 'vitest'
import {
  ENGINE_COMMANDS_SHARED,
  ENGINE_COMMAND_SET_2D,
  ENGINE_COMMAND_SET_3D,
  isCommandAllowedForMotor,
} from './engineCommandCatalog'

describe('engineCommandCatalog', () => {
  it('exposes shared commands', () => {
    expect(ENGINE_COMMANDS_SHARED.length).toBeGreaterThan(0)
    expect(ENGINE_COMMANDS_SHARED).toContain('ping')
  })

  it('builds distinct 2D and 3D command sets', () => {
    expect(ENGINE_COMMAND_SET_2D.has('set_camera_2d')).toBe(true)
    expect(ENGINE_COMMAND_SET_3D.has('set_camera_2d')).toBe(false)
    expect(ENGINE_COMMAND_SET_3D.has('spawn_sun')).toBe(true)
  })

  it('allows set_locale without an active motor', () => {
    expect(isCommandAllowedForMotor('set_locale', null)).toBe(true)
    expect(isCommandAllowedForMotor('ping', null)).toBe(false)
  })
})
