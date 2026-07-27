import { describe, expect, it } from 'vitest'
import { masterPasswordConfirmationError } from '../masterPasswordSetup'

describe('masterPasswordConfirmationError', () => {
  it('requires both password fields', () => {
    expect(masterPasswordConfirmationError('', '')).toBe('主密码不能为空')
    expect(masterPasswordConfirmationError('secret', '')).toBe('请再次输入主密码')
  })

  it('rejects different passwords', () => {
    expect(masterPasswordConfirmationError('secret', 'Secret'))
      .toBe('两次输入的主密码不一致')
  })

  it('accepts matching passwords', () => {
    expect(masterPasswordConfirmationError('secret', 'secret')).toBeNull()
  })
})
