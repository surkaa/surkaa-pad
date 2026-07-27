export type BiometricToggleAction = 'enable' | 'disable' | 'none'

export function biometricToggleAction(
  enabled: boolean,
  requestedValue: boolean,
  busy: boolean,
): BiometricToggleAction {
  if (busy || enabled === requestedValue) return 'none'
  return requestedValue ? 'enable' : 'disable'
}
