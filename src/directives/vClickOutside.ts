import type { Directive, DirectiveBinding } from 'vue'

interface ClickOutsideElement extends HTMLElement {
    _clickOutside?: (event: MouseEvent) => void
}

const clickOutsideDirective: Directive = {
    mounted(el: ClickOutsideElement, binding: DirectiveBinding) {
        // 定义点击外部处理函数
        const handler = (event: MouseEvent) => {
            // 检查点击是否在元素外部
            if (!el.contains(event.target as Node) && el !== event.target) {
                // 调用绑定的方法
                binding.value(event)
            }
        }

        // 保存处理函数引用，以便卸载时使用
        el._clickOutside = handler

        // 添加事件监听器
        // 使用 setTimeout 确保在下一个事件循环中添加，避免立即触发
        setTimeout(() => {
            document.addEventListener('click', handler)
        }, 0)
    },

    unmounted(el: ClickOutsideElement) {
        // 移除事件监听器
        if (el._clickOutside) {
            document.removeEventListener('click', el._clickOutside)
            delete el._clickOutside
        }
    }
}

// 也可以导出指令对象，供局部注册使用
export default clickOutsideDirective;