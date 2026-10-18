import {beforeEach, describe, expect, it, vi} from 'vitest';

const {invokeMock} = vi.hoisted(() => ({
  invokeMock: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
}));

import {biometricCipher} from '../biometric.ts';

describe('biometricCipher', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it('passes encryption data to the official plugin command', async () => {
    invokeMock.mockResolvedValue({data: 'encrypted'});

    await expect(biometricCipher('启用快速解锁', {
      dataToEncrypt: 'plain',
    })).resolves.toEqual({data: 'encrypted'});

    expect(invokeMock).toHaveBeenCalledWith(
        'plugin:biometric|biometric_cipher',
        {reason: '启用快速解锁', dataToEncrypt: 'plain'},
    );
  });

  it('passes decryption data to the official plugin command', async () => {
    invokeMock.mockResolvedValue({data: 'plain'});

    await biometricCipher('解锁日记', {dataToDecrypt: 'encrypted'});

    expect(invokeMock).toHaveBeenCalledWith(
        'plugin:biometric|biometric_cipher',
        {reason: '解锁日记', dataToDecrypt: 'encrypted'},
    );
  });

  it('preserves stored ciphertext unchanged when requesting decryption', async () => {
    const ciphertext = 'YWVzLWdjbS1jaXBoZXJ0ZXh0;MTIzNDU2Nzg5MDEy';
    invokeMock.mockResolvedValue({data: 'plain'});

    await expect(biometricCipher('解锁日记', {dataToDecrypt: ciphertext}))
        .resolves.toEqual({data: 'plain'});

    expect(invokeMock).toHaveBeenCalledExactlyOnceWith(
        'plugin:biometric|biometric_cipher',
        {reason: '解锁日记', dataToDecrypt: ciphertext},
    );
  });

  it('propagates authentication rejection without returning decrypted data', async () => {
    const error = {code: 'userCancel', message: 'The user canceled authentication'};
    invokeMock.mockRejectedValue(error);

    await expect(biometricCipher('解锁日记', {dataToDecrypt: 'encrypted'}))
        .rejects.toBe(error);
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });
});
