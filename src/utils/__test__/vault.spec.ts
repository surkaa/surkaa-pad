import {describe, expect, it} from 'vitest';
import {
  isVaultVerifierValid,
  requiresVaultLogin,
  VAULT_VERIFIER_TEXT,
} from '../vault';

describe('isVaultVerifierValid', () => {
  it('accepts only the current vault verifier text', () => {
    expect(isVaultVerifierValid(VAULT_VERIFIER_TEXT)).toBe(true);
    expect(isVaultVerifierValid('')).toBe(false);
    expect(isVaultVerifierValid('surkaa-pad:vault-verifier:v2')).toBe(false);
  });
});

describe('requiresVaultLogin', () => {
  it('does not use the OSS config as the only initialized-vault marker', () => {
    expect(requiresVaultLogin(true, false, false)).toBe(true);
    expect(requiresVaultLogin(false, true, false)).toBe(true);
    expect(requiresVaultLogin(false, false, true)).toBe(true);
    expect(requiresVaultLogin(false, false, false)).toBe(false);
  });
});
