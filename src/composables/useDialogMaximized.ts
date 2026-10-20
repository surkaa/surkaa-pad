import {computed, ref, toValue, watch, type MaybeRefOrGetter} from 'vue';
import {useQuasar} from 'quasar';

/** 窄屏自动全屏；手动全屏只在本次弹窗打开期间生效。 */
export function useDialogMaximized(visible: MaybeRefOrGetter<boolean>) {
  const $q = useQuasar();
  const manuallyMaximized = ref(false);
  const autoMaximized = computed(() => $q.screen.lt.sm);
  const isMaximized = computed(() => autoMaximized.value || manuallyMaximized.value);

  watch(() => toValue(visible), shown => {
    // 打开时复位，避免全屏弹窗在关闭动画中先缩回普通尺寸。
    if (shown) manuallyMaximized.value = false;
  });

  function toggleMaximized() {
    manuallyMaximized.value = !manuallyMaximized.value;
  }

  return {autoMaximized, isMaximized, toggleMaximized};
}
