import {onBeforeUnmount, ref, shallowRef, toValue, watch, type MaybeRefOrGetter} from 'vue';
import {fetchJsonlPage, type JsonlLine} from '../utils/jsonlPreview';
import {formatError} from '../utils/formatError';

export function useJsonlPreview(
  url: MaybeRefOrGetter<string | undefined>,
  loader: typeof fetchJsonlPage = fetchJsonlPage,
) {
  const lines = shallowRef<JsonlLine[]>([]);
  const loading = ref(false);
  const finished = ref(false);
  const error = ref('');
  let cursor: string | null = null;
  let revision = 0;
  let controller: AbortController | undefined;

  function cancel() { revision++; controller?.abort(); controller = undefined; loading.value = false; }

  async function loadMore() {
    const currentUrl = toValue(url);
    if (!currentUrl || loading.value || finished.value) return;
    const expectedRevision = revision;
    const currentController = new AbortController();
    controller = currentController;
    loading.value = true;
    error.value = '';
    try {
      const page = await loader(currentUrl, cursor, currentController.signal);
      if (expectedRevision !== revision || currentController.signal.aborted) return;
      const expectedLine = (lines.value[lines.value.length - 1]?.lineNumber ?? 0) + 1;
      if (page.lines.length && page.lines[0].lineNumber !== expectedLine) {
        throw new Error('JSONL 行号不连续，请重新打开预览');
      }
      lines.value = [...lines.value, ...page.lines];
      cursor = page.nextCursor;
      finished.value = cursor === null;
    } catch (cause) {
      if (expectedRevision === revision && !currentController.signal.aborted) error.value = formatError(cause);
    } finally {
      if (expectedRevision === revision) { loading.value = false; controller = undefined; }
    }
  }

  watch(() => toValue(url), () => {
    cancel(); lines.value = []; cursor = null; finished.value = false; error.value = '';
    void loadMore();
  }, {immediate: true});
  onBeforeUnmount(cancel);
  return {lines, loading, finished, error, loadMore};
}
