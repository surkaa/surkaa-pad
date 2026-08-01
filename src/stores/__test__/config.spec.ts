// @vitest-environment happy-dom
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { effectScope, nextTick } from 'vue'
import { createPinia, setActivePinia } from 'pinia'

import { useConfigStore } from '../config'

const STORAGE_PREFIX = 'config:'

function clearConfigStorage() {
    for (let i = localStorage.length - 1; i >= 0; i--) {
        const key = localStorage.key(i)
        if (key && key.startsWith(STORAGE_PREFIX)) {
            localStorage.removeItem(key)
        }
    }
}

describe('config store', () => {
    beforeEach(() => {
        setActivePinia(createPinia())
        clearConfigStorage()
    })

    describe('saveNormalConfig + getNormalConfig', () => {
        it('returns default value when key is not set', async () => {
            const store = useConfigStore()
            const val = await store.getNormalConfig('app-theme')
            expect(val).toBe('system')
        })

        it('defaults every attachment type to encrypted', async () => {
            const store = useConfigStore()

            await expect(store.getNormalConfig('encrypt_image_attachments')).resolves.toBe(true)
            await expect(store.getNormalConfig('encrypt_audio_attachments')).resolves.toBe(true)
            await expect(store.getNormalConfig('encrypt_video_attachments')).resolves.toBe(true)
            await expect(store.getNormalConfig('encrypt_file_attachments')).resolves.toBe(true)
        })

        it('defaults attachment upload concurrency to five', async () => {
            const store = useConfigStore()

            await expect(store.getNormalConfig('attachment_upload_concurrency')).resolves.toBe(5)
        })

        it('defaults diary list navigation shortcuts', async () => {
            const store = useConfigStore()

            await expect(store.getNormalConfig('windows_diary_list_shortcuts')).resolves.toEqual({
                createDiary: 'Ctrl+KeyN',
                aiAssistant: 'Ctrl+Alt+KeyA',
                search: 'Ctrl+KeyF',
                settings: 'Ctrl+Comma',
            })
        })

        it('fills shortcuts added after an older list shortcut config was saved', async () => {
            localStorage.setItem(
                `${STORAGE_PREFIX}windows_diary_list_shortcuts`,
                JSON.stringify({search: 'Ctrl+Alt+KeyS', settings: ''}),
            )
            const store = useConfigStore()

            await expect(store.getNormalConfig('windows_diary_list_shortcuts')).resolves.toEqual({
                createDiary: 'Ctrl+KeyN',
                aiAssistant: 'Ctrl+Alt+KeyA',
                search: 'Ctrl+Alt+KeyS',
                settings: '',
            })
        })

        it('normalizes attachment upload concurrency to the supported range', async () => {
            const store = useConfigStore()

            await store.saveNormalConfig('attachment_upload_concurrency', 99)
            await expect(store.getNormalConfig('attachment_upload_concurrency')).resolves.toBe(20)

            localStorage.setItem(
                `${STORAGE_PREFIX}attachment_upload_concurrency`,
                JSON.stringify('invalid'),
            )
            await expect(store.getNormalConfig('attachment_upload_concurrency')).resolves.toBe(5)
        })

        it('persists a disabled attachment encryption preference', async () => {
            const store = useConfigStore()

            await store.saveNormalConfig('encrypt_audio_attachments', false)

            await expect(store.getNormalConfig('encrypt_audio_attachments')).resolves.toBe(false)
            await expect(store.getNormalConfig('encrypt_video_attachments')).resolves.toBe(true)
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

        it('reads and removes the legacy remote storage preference', async () => {
            const store = useConfigStore()
            localStorage.setItem(`${STORAGE_PREFIX}remote_enabled`, 'false')

            await expect(store.getLegacyRemoteEnabled()).resolves.toBe(false)
            await store.deleteLegacyRemoteEnabled()

            expect(localStorage.getItem(`${STORAGE_PREFIX}remote_enabled`)).toBeNull()
        })

        it('infers the legacy remote preference from an OSS config when the flag is missing', async () => {
            const store = useConfigStore()
            localStorage.setItem(`${STORAGE_PREFIX}encrypted_oss_config`, '[1,2,3]')

            await expect(store.getLegacyRemoteEnabled()).resolves.toBe(true)
        })

        it('defaults the last password unlock time to missing and persists it', async () => {
            const store = useConfigStore()
            await expect(store.getNormalConfig('last_password_unlock_at')).resolves.toBeNull()

            await store.saveNormalConfig('last_password_unlock_at', 1_700_000_000_000)

            await expect(store.getNormalConfig('last_password_unlock_at'))
                .resolves.toBe(1_700_000_000_000)
        })

        it('stores an encrypted verifier independently from the OSS config', async () => {
            const store = useConfigStore()

            await expect(store.getNormalConfig('vault_verifier')).resolves.toBeNull()
            await store.saveNormalConfig('vault_verifier', [4, 5, 6])

            await expect(store.getNormalConfig('vault_verifier')).resolves.toEqual([4, 5, 6])
            await expect(store.getNormalConfig('encrypted_oss_config')).resolves.toBeNull()
        })

        it('keeps the encrypted AI config separate from other secrets', async () => {
            const store = useConfigStore()

            await expect(store.getNormalConfig('encrypted_ai_config')).resolves.toBeNull()
            await store.saveNormalConfig('encrypted_ai_config', [7, 8, 9])

            await expect(store.getNormalConfig('encrypted_ai_config')).resolves.toEqual([7, 8, 9])
            await expect(store.getNormalConfig('encrypted_oss_config')).resolves.toBeNull()
            await expect(store.getNormalConfig('vault_verifier')).resolves.toBeNull()
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

        it('notifies other instances in same window when setter writes', async () => {
            const store = useConfigStore()
            const refA = store.useTauriConfig('app-theme')
            const refB = store.useTauriConfig('app-theme')

            refA.value = 'dark'

            await nextTick()
            expect(refB.value).toBe('dark')
        })

        it('saveNormalConfig notifies useTauriConfig instances', async () => {
            const store = useConfigStore()
            const ref = store.useTauriConfig('app-theme')
            expect(ref.value).toBe('system')

            await store.saveNormalConfig('app-theme', 'light')

            await nextTick()
            expect(ref.value).toBe('light')
        })

        it('deleteConfig notifies useTauriConfig instances', async () => {
            const store = useConfigStore()
            const ref = store.useTauriConfig('app-theme')
            ref.value = 'dark'
            expect(ref.value).toBe('dark')

            await store.deleteConfig('app-theme')

            await nextTick()
            expect(ref.value).toBe('system')
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

            scope.run(() => {
                const store = useConfigStore()
                void store.useTauriConfig('app-theme')
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
