import {getIdentifier, getName} from '@tauri-apps/api/app';
import {onBeforeUnmount, onMounted, readonly, ref} from 'vue';
import {isDevelopmentEdition} from '../utils/vaultKdfSetup';

/** Dev 安装包虽采用 Release 构建，仍应保留开发测试专用选项。 */
export function useDevelopmentEdition() {
  const development = ref(import.meta.env.DEV);
  let active = true;
  onBeforeUnmount(() => { active = false; });
  onMounted(async () => {
    if (development.value) return;
    try {
      const [identifier, name] = await Promise.all([getIdentifier(), getName()]);
      if (active) development.value = isDevelopmentEdition(identifier, name, false);
    } catch {
      // 读取失败时保持正式版选项，不能因无法确认版本而开放低成本派生。
    }
  });
  return readonly(development);
}
