export const PASSWORD_UNLOCK_VALIDITY_MS = 7 * 24 * 60 * 60 * 1000;

export function canUseBiometricUnlock(
    lastPasswordUnlockAt: number | null,
    now = Date.now(),
): boolean {
  if (lastPasswordUnlockAt === null
      || !Number.isFinite(lastPasswordUnlockAt)
      || !Number.isFinite(now)) return false;

  const elapsed = now - lastPasswordUnlockAt;
  return elapsed >= 0 && elapsed < PASSWORD_UNLOCK_VALIDITY_MS;
}
