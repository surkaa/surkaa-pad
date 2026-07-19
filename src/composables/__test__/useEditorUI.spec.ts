import { beforeEach, describe, expect, it, vi } from 'vitest'

const { platform } = vi.hoisted(() => ({ platform: vi.fn() }))

vi.mock('@tauri-apps/plugin-os', () => ({ platform }))

import { useEditorUI } from '../useEditorUI'

describe('useEditorUI', () => {
  beforeEach(() => {
    platform.mockReset()
    platform.mockReturnValue('android')
  })

  it('keeps the Android toolbar hidden until the editor is first focused', () => {
    const { showToolbar, setupToolbar, showToolbarAfterEditorFocus } = useEditorUI()

    setupToolbar()
    expect(showToolbar.value).toBe(false)

    showToolbarAfterEditorFocus()
    expect(showToolbar.value).toBe(true)
  })
})
