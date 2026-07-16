export interface LongPressController {
  start(action: () => void): void
  cancel(): void
  consumeTriggered(): boolean
}

export function createLongPressController(delay = 500): LongPressController {
  let timer: ReturnType<typeof setTimeout> | null = null
  let triggered = false

  return {
    start(action) {
      if (timer) clearTimeout(timer)
      triggered = false
      timer = setTimeout(() => {
        timer = null
        triggered = true
        action()
      }, delay)
    },
    cancel() {
      if (timer) {
        clearTimeout(timer)
        timer = null
      }
    },
    consumeTriggered() {
      const result = triggered
      triggered = false
      return result
    },
  }
}
