export const VAULT_VERIFIER_TEXT = 'surkaa-pad:vault-verifier:v1';

export function isVaultVerifierValid(value: string): boolean {
  return value === VAULT_VERIFIER_TEXT;
}

export function requiresVaultLogin(
  hasVerifier: boolean,
  hasEncryptedOssConfig: boolean,
  hasLegacyLocalDiary: boolean,
): boolean {
  return hasVerifier || hasEncryptedOssConfig || hasLegacyLocalDiary;
}
