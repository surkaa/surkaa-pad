export const DEFAULT_PASSWORD_UNLOCK_INTERVAL_DAYS = 7;
export const PASSWORD_UNLOCK_INTERVAL_OPTIONS_DAYS = [1, 3, 7, 14, 30] as const;
export const PASSWORD_UNLOCK_VALIDITY_MS = DEFAULT_PASSWORD_UNLOCK_INTERVAL_DAYS * 24 * 60 * 60 * 1000;

export function normalizePasswordUnlockIntervalDays(value: unknown): number {
  return typeof value === 'number'
      && Number.isInteger(value)
      && PASSWORD_UNLOCK_INTERVAL_OPTIONS_DAYS.includes(
        value as typeof PASSWORD_UNLOCK_INTERVAL_OPTIONS_DAYS[number],
      )
    ? value
    : DEFAULT_PASSWORD_UNLOCK_INTERVAL_DAYS;
}

export function passwordUnlockValidityMs(intervalDays = DEFAULT_PASSWORD_UNLOCK_INTERVAL_DAYS): number {
  return normalizePasswordUnlockIntervalDays(intervalDays) * 24 * 60 * 60 * 1000;
}

export function canUseBiometricUnlock(
    lastPasswordUnlockAt: number | null,
    now = Date.now(),
    validityMs = PASSWORD_UNLOCK_VALIDITY_MS,
): boolean {
  if (lastPasswordUnlockAt === null
      || !Number.isFinite(lastPasswordUnlockAt)
      || !Number.isFinite(now)) return false;

  const elapsed = now - lastPasswordUnlockAt;
  return elapsed >= 0 && elapsed < validityMs;
}
