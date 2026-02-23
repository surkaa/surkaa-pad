import {EventBus} from "quasar";
import {onMounted, onUnmounted} from "vue";
import {DiarySummary} from "../bindings.ts";

const bus = new EventBus();

export type DiaryChangedEvent =
    | { type: 'created'; summary: DiarySummary }
    | { type: 'updated'; summary: DiarySummary }
    | { type: 'deleted'; id: string };

// 定义需要用到事件及其回调函数的类型
export type EventCallbacks = {
    'diary-changed': (event: DiaryChangedEvent) => void;
}

export function eventBusOn<K extends keyof EventCallbacks>(event: K, callback: EventCallbacks[K]): void {
    bus.on(event, callback);
}

export function eventBusOff<K extends keyof EventCallbacks>(event: K, callback: EventCallbacks[K]): void {
    bus.off(event, callback);
}

export function eventBusEmit<K extends keyof EventCallbacks>(event: K, payload: Parameters<EventCallbacks[K]>[0]): void {
    bus.emit(event, payload);
}

export function eventBusOnce<K extends keyof EventCallbacks>(event: K, callback: EventCallbacks[K]): void {
    bus.once(event, callback);
}

export function useEventBus<K extends keyof EventCallbacks>(event: K, callback: EventCallbacks[K]): void {
    onMounted(() => {
        bus.on(event, callback);
    });
    onUnmounted(() => {
        bus.off(event, callback);
    });
}
