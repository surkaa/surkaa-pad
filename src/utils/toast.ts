export type ToastType = 'info' | 'success' | 'error' | 'warning';

interface ToastOptions {
    type?: ToastType;
    duration?: number;
    position?: 'top-center' | 'top-right' | 'top-left' | 'bottom-center' | 'bottom-right' | 'bottom-left';
    dismissible?: boolean;
    icon?: boolean;
}

const DEFAULT_POSITION = 'bottom-center';

let activeToasts: Array<{toast: HTMLDivElement; timeoutId: number | null}> = [];

function getContainer(position: ToastOptions['position'] = DEFAULT_POSITION) {
    const positionClass = position.replace('-', '-') + '-toast-container';
    let container = document.querySelector(`.${positionClass}`);
    if (!container) {
        container = document.createElement('div');
        container.className = `toast-container ${positionClass}`;
        document.body.appendChild(container);
    }
    return container;
}

/**
 * 获取 Toast 图标
 */
function getToastIcon(type: ToastType): string {
    switch (type) {
        case 'success':
            return `
                    <svg class="toast-icon success" viewBox="0 0 24 24" width="20" height="20">
                      <path fill="currentColor" d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z"/>
                    </svg>
                  `;
        case 'error':
            return `
                    <svg class="toast-icon error" viewBox="0 0 24 24" width="20" height="20">
                      <path fill="currentColor" d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/>
                    </svg>
                  `;
        case 'warning':
            return `
                    <svg class="toast-icon warning" viewBox="0 0 24 24" width="20" height="20">
                      <path fill="currentColor" d="M1 21h22L12 2 1 21zm12-3h-2v-2h2v2zm0-4h-2v-4h2v4z"/>
                    </svg>
                  `;
        case 'info':
        default:
            return `
                    <svg class="toast-icon info" viewBox="0 0 24 24" width="20" height="20">
                      <path fill="currentColor" d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z"/>
                    </svg>
                  `;
    }
}

/**
 * 显示Toast消息
 * @param message  消息内容
 * @param type     消息类型: 'info' | 'success' | 'error' | 'warning'
 * @param duration 显示时长，默认3000毫秒
 * @param options  额外选项
 */
export function showToast(
    message: string,
    type: ToastType = 'success',
    duration = 3000,
    options: ToastOptions = {}
) {
    const {
        position = DEFAULT_POSITION,
        dismissible = true,
        icon = true
    } = options;

    const container = getContainer(position);
    const toast = document.createElement('div');
    toast.className = `toast ${type}`;
    toast.setAttribute('role', 'alert');
    toast.setAttribute('aria-live', 'assertive');
    toast.setAttribute('aria-atomic', 'true');

    const iconHtml = icon ? getToastIcon(type) : '';
    const closeButtonHtml = dismissible ? `
        <button class="toast-close" aria-label="关闭提示">
          <svg viewBox="0 0 24 24" width="16" height="16">
            <path fill="currentColor" d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/>
          </svg>
        </button>` : '';

    toast.innerHTML = `
        <div class="toast-content">
          ${iconHtml}
          <div class="toast-message">${message}</div>
        </div>
        ${closeButtonHtml}
      `;

    container.appendChild(toast);

    // 添加进入动画
    requestAnimationFrame(() => {
        toast.classList.add('show');
    });

    // 点击关闭
    if (dismissible) {
        const closeBtn = toast.querySelector('.toast-close');
        if (closeBtn) {
            closeBtn.addEventListener('click', () => {
                removeToast(toast);
            });
        }
    }

    // 自动移除
    let timeoutId: number | null = null;
    if (duration > 0) {
        timeoutId = window.setTimeout(() => {
            removeToast(toast);
        }, duration);
    }

    // 悬停暂停自动关闭
    toast.addEventListener('mouseenter', () => {
        if (duration > 0 && timeoutId) {
            clearTimeout(timeoutId);
        }
    });

    toast.addEventListener('mouseleave', () => {
        if (duration > 0) {
            timeoutId = window.setTimeout(() => {
                removeToast(toast);
            }, duration);
        }
    });

    // 添加到全局管理
    activeToasts.push({ toast, timeoutId });
}

/**
 * 移除Toast
 */
function removeToast(toast: HTMLDivElement) {
    toast.classList.remove('show');
    toast.classList.add('hide');

    // 等待动画结束后移除
    setTimeout(() => {
        if (toast.parentElement) {
            toast.remove();
        }
    }, 300);
}

/**
 * 清除所有Toast
 */
export function clearAllToasts() {
    const containers = document.querySelectorAll('.toast-container');
    containers.forEach(container => {
        container.innerHTML = '';
    });

    for (let i = 0; i < activeToasts.length; i++){
        const item = activeToasts[i];
        if (item.timeoutId) {
            clearTimeout(item.timeoutId);
        }
    }
    activeToasts = [];
}