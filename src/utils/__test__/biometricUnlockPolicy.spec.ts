import {describe, expect, it} from 'vitest';
import {
  canUseBiometricUnlock,
  PASSWORD_UNLOCK_VALIDITY_MS,
} from '../biometricUnlockPolicy.ts';

describe('canUseBiometricUnlock', () => {
  const now = 1_800_000_000_000;

  it('allows biometric unlock before seven full days have elapsed', () => {
    expect(canUseBiometricUnlock(now, now)).toBe(true);
    expect(canUseBiometricUnlock(now - PASSWORD_UNLOCK_VALIDITY_MS + 1, now)).toBe(true);
  });

  it('requires a password at exactly seven days and afterwards', () => {
    expect(canUseBiometricUnlock(now - PASSWORD_UNLOCK_VALIDITY_MS, now)).toBe(false);
    expect(canUseBiometricUnlock(now - PASSWORD_UNLOCK_VALIDITY_MS - 1, now)).toBe(false);
  });

  it('requires a password when no previous password unlock was recorded', () => {
    expect(canUseBiometricUnlock(null, now)).toBe(false);
  });

  it('requires a password when the system clock moved behind the recorded time', () => {
    expect(canUseBiometricUnlock(now + 1, now)).toBe(false);
  });

  it('rejects invalid timestamps', () => {
    expect(canUseBiometricUnlock(Number.NaN, now)).toBe(false);
    expect(canUseBiometricUnlock(Number.POSITIVE_INFINITY, now)).toBe(false);
    expect(canUseBiometricUnlock(now, Number.NaN)).toBe(false);
  });
});
