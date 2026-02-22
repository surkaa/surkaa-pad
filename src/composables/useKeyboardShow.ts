import {Ref, ref} from "vue";
import {useEventListener} from "@vueuse/core";

export function useKeyboardShow(
    showRef: Ref<boolean>,
    heightThreshold = 150,
    minHeightDifference = 50
) {
    if (!window.visualViewport) return null;
    const initialHeight = ref(window.innerHeight);
    const diff = ref(0);

    useEventListener('resize', () => {
        if (!window.visualViewport) return;
        const currentHeight = window.visualViewport.height;
        diff.value = initialHeight.value - currentHeight;
        if (diff.value > heightThreshold) {
            if (!showRef.value) {
                showRef.value = true;
            }
        } else {
            // 允许 minHeightDifference 的误差（处理导航栏隐藏/显示的抖动）
            if (diff.value < minHeightDifference && showRef.value) {
                showRef.value = false;
            }
        }
    });
}