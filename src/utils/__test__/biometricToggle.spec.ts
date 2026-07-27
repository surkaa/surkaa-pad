import { describe, expect, it } from 'vitest'
import { biometricToggleAction } from '../biometricToggle'

describe('biometricToggleAction', () => {
  it('requests setup without treating the feature as already enabled', () => {
    expect(biometricToggleAction(false, true, false)).toBe('enable')
  })

  it('requests confirmation before disabling', () => {
    expect(biometricToggleAction(true, false, false)).toBe('disable')
  })

  it('ignores unchanged values and input while setup is busy', () => {
    expect(biometricToggleAction(false, false, false)).toBe('none')
    expect(biometricToggleAction(false, true, true)).toBe('none')
  })
})
