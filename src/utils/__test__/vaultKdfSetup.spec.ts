import {describe, expect, it} from 'vitest';
import {defaultNewVaultMemoryCost, isDevelopmentEdition, newVaultMemoryOptions} from '../vaultKdfSetup';

describe('vaultKdfSetup', () => {
  it('keeps the development default inexpensive without exposing it in release', () => {
    expect(defaultNewVaultMemoryCost(true)).toBe(1024);
    expect(defaultNewVaultMemoryCost(false)).toBe(256 * 1024);
    expect(newVaultMemoryOptions(true).map(option => option.value)).toEqual([
      1024,
      64 * 1024,
      128 * 1024,
      256 * 1024,
    ]);
    expect(newVaultMemoryOptions(false).map(option => option.value)).toEqual([
      64 * 1024,
      128 * 1024,
      256 * 1024,
    ]);
  });

  it.each([
    ['cn.surkaa.pad.dev', 'SurKaa Pad (Dev)', false, true],
    ['cn.surkaa.pad.dev', 'SurKaa Pad', false, true],
    ['cn.surkaa.pad', 'SurKaa Pad (Dev)', false, true],
    ['cn.surkaa.pad', 'SurKaa Pad', true, true],
    ['cn.surkaa.pad', 'SurKaa Pad', false, false],
    ['', '', false, false],
    ['cn.surkaa.pad.development', 'SurKaa Pad (Dev) backup', false, false],
  ])('detects edition for identifier=%s name=%s frontendDev=%s', (identifier, name, frontendDev, expected) => {
    expect(isDevelopmentEdition(identifier, name, frontendDev)).toBe(expected);
    expect(newVaultMemoryOptions(expected).some(option => option.value === 1024)).toBe(expected);
  });
});
