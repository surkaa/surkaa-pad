// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {clearMocks, mockIPC} from '@tauri-apps/api/mocks';
import {biometricCipher} from '@tauri-apps/plugin-biometric';

const invokeMock = vi.fn();

describe('official biometricCipher integration', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    mockIPC(invokeMock);
  });

  afterEach(() => clearMocks());

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
