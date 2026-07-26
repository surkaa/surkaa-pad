export async function runDiaryDeletion(
  deleteFromStorage: () => Promise<unknown>,
  onDeleted: () => void,
  onError: (error: unknown) => void,
): Promise<boolean> {
  try {
    await deleteFromStorage()
  } catch (error) {
    onError(error)
    return false
  }

  onDeleted()
  return true
}
