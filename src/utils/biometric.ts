import {invoke} from '@tauri-apps/api/core';

export interface BiometricCipherOptions {
  /** 成功验证生物识别后需要加密的明文。 */
  dataToEncrypt?: string;
  /** 成功验证生物识别后需要解密的密文。 */
  dataToDecrypt?: string;
}

/**
 * 调用 Android 生物识别插件中由项目扩展的加解密命令。
 */
export async function biometricCipher(
    reason: string,
    options: BiometricCipherOptions = {},
): Promise<{data: string}> {
  return invoke<{data: string}>('plugin:biometric|biometric_cipher', {
    reason,
    ...options,
  });
}
