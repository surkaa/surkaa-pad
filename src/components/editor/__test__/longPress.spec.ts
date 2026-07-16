import { afterEach, describe, expect, it, vi } from 'vitest'
import { createLongPressController } from '../longPress'

describe('createLongPressController', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  it('triggers after the configured delay and consumes the result once', () => {
    vi.useFakeTimers()
    const action = vi.fn()
    const controller = createLongPressController(500)

    controller.start(action)
    vi.advanceTimersByTime(499)
    expect(action).not.toHaveBeenCalled()
    vi.advanceTimersByTime(1)

    expect(action).toHaveBeenCalledOnce()
    expect(controller.isTriggered()).toBe(true)
    expect(controller.consumeTriggered()).toBe(true)
    expect(controller.isTriggered()).toBe(false)
    expect(controller.consumeTriggered()).toBe(false)
  })

  it('does not trigger after cancellation', () => {
    vi.useFakeTimers()
    const action = vi.fn()
    const controller = createLongPressController(500)

    controller.start(action)
    controller.cancel()
    vi.advanceTimersByTime(500)

    expect(action).not.toHaveBeenCalled()
    expect(controller.isTriggered()).toBe(false)
    expect(controller.consumeTriggered()).toBe(false)
  })
})
