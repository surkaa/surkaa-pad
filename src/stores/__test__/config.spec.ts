// @vitest-environment happy-dom
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { effectScope, nextTick } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

vi.mock('@tauri-apps/plugin-store', () => {
    const mockStore = {
        length: vi.fn().mockResolvedValue(0),
        get: vi.fn().mockResolvedValue(null),
        set: vi.fn().mockResolvedValue(undefined),
        save: vi.fn().mockResolvedValue(undefined),
        onKeyChange: vi.fn().mockResolvedValue(vi.fn()),
    }
    return {
        Store: {
            load: vi.fn().mockResolvedValue(mockStore),
        },
    }
})

import { useConfigStore } from '../config'

const STORAGE_PREFIX = 'config:'
const MIGRATION_KEY = 'config:migrated'

function clearConfigStorage() {
    for (let i = localStorage.length - 1; i >= 0; i--) {
        const key = localStorage.key(i)
        if (key && key.startsWith(STORAGE_PREFIX)) {
            localStorage.removeItem(key)
        }
    }
    localStorage.removeItem(MIGRATION_KEY)
}

describe('config store', () => {
    beforeEach(() => {
        setActivePinia(createPinia())
        clearConfigStorage()
        // 模拟已迁移完成，跳过 tauri-plugin-store 调用
        localStorage.setItem(MIGRATION_KEY, 'true')
    })

    describe('saveNormalConfig + getNormalConfig', () => {
        it('returns default value when key is not set', async () => {
            const store = useConfigStore()
            const val = await store.getNormalConfig('app-theme')
            expect(val).toBe('system')
        })

        it('saves and retrieves a value', async () => {
            const store = useConfigStore()
            await store.saveNormalConfig('app-theme', 'dark')
            const val = await store.getNormalConfig('app-theme')
            expect(val).toBe('dark')
        })

        it('saves and retrieves boolean', async () => {
            const store = useConfigStore()
            await store.saveNormalConfig('biometric_enabled', true)
            expect(await store.getNormalConfig('biometric_enabled')).toBe(true)
        })

        it('saves and retrieves number[]', async () => {
            const store = useConfigStore()
            await store.saveNormalConfig('encrypted_oss_config', [1, 2, 3])
            expect(await store.getNormalConfig('encrypted_oss_config')).toEqual([1, 2, 3])
        })

        it('saves and retrieves string array', async () => {
            const store = useConfigStore()
            await store.saveNormalConfig('pinned_diary_ids', ['a', 'b'])
            expect(await store.getNormalConfig('pinned_diary_ids')).toEqual(['a', 'b'])
        })

        it('persists across store instances', async () => {
            const store1 = useConfigStore()
            await store1.saveNormalConfig('app-theme', 'light')

            const store2 = useConfigStore()
            expect(await store2.getNormalConfig('app-theme')).toBe('light')
        })
    })

    describe('deleteConfig', () => {
        it('removes a key, reverting to default', async () => {
            const store = useConfigStore()
            await store.saveNormalConfig('app-theme', 'dark')
            await store.deleteConfig('app-theme')
            expect(await store.getNormalConfig('app-theme')).toBe('system')
        })

        it('removes multiple keys at once', async () => {
            const store = useConfigStore()
            await store.saveNormalConfig('app-theme', 'light')
            await store.saveNormalConfig('biometric_enabled', true)
            await store.deleteConfig('app-theme', 'biometric_enabled')

            expect(await store.getNormalConfig('app-theme')).toBe('system')
            expect(await store.getNormalConfig('biometric_enabled')).toBe(false)
        })
    })

    describe('useTauriConfig', () => {
        it('returns current stored value synchronously', () => {
            localStorage.setItem(`${STORAGE_PREFIX}app-theme`, JSON.stringify('dark'))
            const store = useConfigStore()
            const ref = store.useTauriConfig('app-theme')
            expect(ref.value).toBe('dark')
        })

        it('returns default when no stored value', () => {
            const store = useConfigStore()
            const ref = store.useTauriConfig('biometric_enabled')
            expect(ref.value).toBe(false)
        })

        it('writes to localStorage on set', () => {
            const store = useConfigStore()
            const ref = store.useTauriConfig('app-theme')
            ref.value = 'light'
            expect(localStorage.getItem(`${STORAGE_PREFIX}app-theme`)).toBe(JSON.stringify('light'))
        })

        it('reacts to cross-window storage event', async () => {
            const store = useConfigStore()
            const ref = store.useTauriConfig('app-theme')
            expect(ref.value).toBe('system')

            // 模拟另一个窗口修改了 localStorage
            localStorage.setItem(`${STORAGE_PREFIX}app-theme`, JSON.stringify('dark'))
            window.dispatchEvent(new StorageEvent('storage', {
                key: `${STORAGE_PREFIX}app-theme`,
                newValue: JSON.stringify('dark'),
            }))

            await nextTick()
            expect(ref.value).toBe('dark')
        })

        it('reverts to default when key is removed in another window', async () => {
            const store = useConfigStore()
            const ref = store.useTauriConfig('app-theme')
            ref.value = 'dark'
            expect(ref.value).toBe('dark')

            localStorage.removeItem(`${STORAGE_PREFIX}app-theme`)
            window.dispatchEvent(new StorageEvent('storage', {
                key: `${STORAGE_PREFIX}app-theme`,
                newValue: null,
            }))

            await nextTick()
            expect(ref.value).toBe('system')
        })

        it('handles corrupt JSON in storage event gracefully', async () => {
            const store = useConfigStore()
            const ref = store.useTauriConfig('app-theme')
            ref.value = 'dark'

            window.dispatchEvent(new StorageEvent('storage', {
                key: `${STORAGE_PREFIX}app-theme`,
                newValue: '{broken',
            }))

            await nextTick()
            expect(ref.value).toBe('system') // falls back to default
        })

        it('cleans up storage listener on scope dispose', () => {
            const scope = effectScope()
            let ref: any

            scope.run(() => {
                const store = useConfigStore()
                ref = store.useTauriConfig('app-theme')
            })

            // 获取 addEventListener 的 spy
            const addSpy = vi.spyOn(window, 'addEventListener')
            const removeSpy = vi.spyOn(window, 'removeEventListener')

            scope.stop()

            // 验证 removeEventListener 被调用
            const storageCalls = removeSpy.mock.calls.filter(
                ([event]) => event === 'storage'
            )
            expect(storageCalls.length).toBeGreaterThanOrEqual(1)

            addSpy.mockRestore()
            removeSpy.mockRestore()
        })
    })
})
