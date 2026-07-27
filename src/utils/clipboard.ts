export async function copyTextToClipboard(text: string): Promise<void> {
    let clipboardError: unknown;
    if (navigator.clipboard?.writeText) {
        try {
            await navigator.clipboard.writeText(text);
            return;
        } catch (error) {
            clipboardError = error;
        }
    }

    const textarea = document.createElement('textarea');
    textarea.value = text;
    textarea.setAttribute('readonly', '');
    textarea.style.position = 'fixed';
    textarea.style.opacity = '0';
    document.body.appendChild(textarea);
    textarea.select();
    try {
        if (!document.execCommand('copy')) {
            throw clipboardError ?? new Error('浏览器未能复制文本');
        }
    } finally {
        textarea.remove();
    }
}
