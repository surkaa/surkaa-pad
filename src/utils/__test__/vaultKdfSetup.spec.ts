import {describe, expect, it} from 'vitest';
import {defaultNewVaultMemoryCost, newVaultMemoryOptions} from '../vaultKdfSetup';

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
});
