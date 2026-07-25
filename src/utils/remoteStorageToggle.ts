export type RemoteStorageToggleAction = 'enable' | 'disable' | 'none'

export function remoteStorageToggleAction(
  currentValue: boolean,
  requestedValue: boolean,
  busy: boolean,
): RemoteStorageToggleAction {
  if (busy || currentValue === requestedValue) return 'none'
  return requestedValue ? 'enable' : 'disable'
}
