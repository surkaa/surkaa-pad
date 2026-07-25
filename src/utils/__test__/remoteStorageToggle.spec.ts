import { describe, expect, it } from 'vitest'
import { remoteStorageToggleAction } from '../remoteStorageToggle'

describe('remoteStorageToggleAction', () => {
  it('handles disable followed by a real enable request', () => {
    expect(remoteStorageToggleAction(true, false, false)).toBe('disable')
    expect(remoteStorageToggleAction(false, true, false)).toBe('enable')
  })

  it('ignores duplicate values and requests while syncing', () => {
    expect(remoteStorageToggleAction(true, true, false)).toBe('none')
    expect(remoteStorageToggleAction(false, true, true)).toBe('none')
  })
})
