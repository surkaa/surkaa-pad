export type ToastType = 'info' | 'success' | 'error' | 'warning';

function getContainer() {
    let container = document.querySelector('.toast-container');
    if (!container) {
        container = document.createElement('div');
        container.className = 'toast-container';
        document.body.appendChild(container);
    }
    return container;
}

/**
 * 显示Toast消息
 * @param message  消息内容
 * @param type     消息类型: 'info' | 'success' | 'error' | 'warning'
 * @param duration 显示时长，默认3000毫秒
 */
export function showToast(message: string, type: ToastType = 'success', duration = 3000) {
    const container = getContainer();
    const toast = document.createElement('div');
    toast.className = `toast ${type}`;
    toast.innerHTML = `
                    <span>${message}</span>
                    <button class="toast-close" onclick="this.parentElement.remove()">&times;</button>
                `;

    container.appendChild(toast);

    // 自动移除
    setTimeout(() => {
        if (toast.parentElement) {
            toast.remove();
        }
    }, duration);
}
