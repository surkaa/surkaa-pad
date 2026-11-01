import {watch, toValue, type MaybeRefOrGetter} from 'vue';
import {isTauri, type PluginListener} from '@tauri-apps/api/core';
import {onBackButtonPress} from '@tauri-apps/api/app';
import {platform} from '@tauri-apps/plugin-os';

/** 只在预览可见时接管 Android 返回，关闭后恢复 Tauri 原有的导航/退出行为。 */
export function useAndroidBackButton(enabled: MaybeRefOrGetter<boolean>, onBack: () => void) {
  watch(() => toValue(enabled), (visible, _previous, onCleanup) => {
    if (!visible || !isTauri() || platform() !== 'android') return;

    let active = true;
    let listener: PluginListener | undefined;
    const unregister = (handle: PluginListener) => {
      void handle.unregister().catch(error => console.error('移除附件预览系统返回监听失败：', error));
    };
    onCleanup(() => {
      active = false;
      if (listener) unregister(listener);
    });

    void onBackButtonPress(() => {
      // 关闭、卸载或快速重开后，已排队的旧回调不能操作新预览或底层日记。
      if (active && toValue(enabled)) onBack();
    }).then(handle => {
      if (active) listener = handle;
      else unregister(handle);
    }).catch(error => {
      if (active) console.error('监听附件预览系统返回失败：', error);
    });
  }, {immediate: true, flush: 'sync'});
}
