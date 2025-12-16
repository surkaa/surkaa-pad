import {type Ref, unref, watch} from "vue";

export function useEventListener<K extends keyof WindowEventMap>(
    type: K,
    listener: (this: Window, ev: WindowEventMap[K]) => any,
    options?: boolean | AddEventListenerOptions
): () => void;

export function useEventListener<K extends keyof HTMLElementEventMap>(
    target: Ref<HTMLElement | null | undefined> | HTMLElement,
    type: K,
    listener: (this: HTMLElement, ev: HTMLElementEventMap[K]) => any,
    options?: boolean | AddEventListenerOptions
): () => void;

export function useEventListener<K extends keyof DocumentEventMap>(
    target: Ref<Document> | Document,
    type: K,
    listener: (this: Document, ev: DocumentEventMap[K]) => any,
    options?: boolean | AddEventListenerOptions
): () => void;

export function useEventListener(...args: any[]): () => void {
    if (args.length < 2) {
        throw new Error('最少需要两个参数');
    }
    const target = typeof args[0] === 'string' ? window : args.shift();
    return watch(
        () => unref(target),
        (el, _, onCleanup) => {
            if (!el || typeof el.addEventListener !== 'function') {
                return;
            }
            el.addEventListener(...args);
            onCleanup(() => {
                el.removeEventListener(...args);
            });
        },
        {immediate: true}
    );
}