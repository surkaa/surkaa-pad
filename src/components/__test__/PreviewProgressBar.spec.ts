// @vitest-environment happy-dom
import {afterEach, describe, expect, it} from 'vitest';
import {createApp, defineComponent, h, nextTick, ref, type App} from 'vue';
import {Dark, Quasar} from 'quasar';
import PreviewProgressBar from '../PreviewProgressBar.vue';

let app: App | undefined;
afterEach(() => {app?.unmount(); app = undefined; Dark.set(false); document.body.innerHTML = '';});

describe('preview progress mode transitions', () => {
  it.each([false, true])('starts real progress on a fresh model, then grows without remounting (dark=%s)', async dark => {
    const value = ref<number>();
    const host = document.createElement('div'); document.body.append(host);
    app = createApp(defineComponent({setup: () => () => h(PreviewProgressBar, {value: value.value, rounded: true, size: '8px', class: 'preview-bar'})}));
    app.use(Quasar); Dark.set(dark); app.mount(host); await nextTick();

    const waiting = host.querySelector('.q-linear-progress__model')!;
    expect(waiting.classList.contains('q-linear-progress__model--indeterminate')).toBe(true);
    expect((waiting as HTMLElement).style.transform).toBe('scale3d(1,1,1)');
    expect(host.querySelector('[role="progressbar"]')?.hasAttribute('aria-valuenow')).toBe(false);

    value.value = 0; await nextTick();
    const model = host.querySelector('.q-linear-progress__model') as HTMLElement;
    expect(model).not.toBe(waiting);
    expect(model.classList.contains('q-linear-progress__model--determinate')).toBe(true);
    expect(model.style.transform).toBe('scale3d(0,1,1)');
    expect(host.querySelector('.preview-bar')).not.toBeNull();
    expect(host.querySelector<HTMLElement>('.q-linear-progress')?.style.fontSize).toBe('8px');
    expect(host.querySelector(`.q-linear-progress__track--${dark ? 'dark' : 'light'}`)).not.toBeNull();

    for (const fraction of [.01, .1, .35, .8, 1]) {
      value.value = fraction; await nextTick();
      expect(host.querySelector('.q-linear-progress__model')).toBe(model);
      expect(model.style.transform).toBe(`scale3d(${fraction},1,1)`);
      expect(host.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow')).toBe(String(fraction));
    }
    expect(host.querySelector<HTMLElement>('.q-linear-progress')?.style.getPropertyValue('--q-linear-progress-speed')).toBe('200ms');

    value.value = undefined; await nextTick();
    const verifying = host.querySelector('.q-linear-progress__model');
    expect(verifying).not.toBe(model);
    expect(verifying?.classList.contains('q-linear-progress__model--indeterminate')).toBe(true);
    value.value = .02; await nextTick();
    const restarted = host.querySelector<HTMLElement>('.q-linear-progress__model');
    expect(restarted).not.toBe(verifying);
    expect(restarted?.style.transform).toBe('scale3d(0.02,1,1)');
  });

  it('also isolates the first nonzero update and treats nonfinite values as unknown', async () => {
    const value = ref<number>(NaN);
    const host = document.createElement('div'); document.body.append(host);
    app = createApp(defineComponent({setup: () => () => h(PreviewProgressBar, {value: value.value})}));
    app.use(Quasar).mount(host);
    const waiting = host.querySelector('.q-linear-progress__model');
    value.value = .05; await nextTick();
    expect(host.querySelector('.q-linear-progress__model')).not.toBe(waiting);
    expect(host.querySelector<HTMLElement>('.q-linear-progress__model')?.style.transform).toBe('scale3d(0.05,1,1)');
    value.value = Infinity; await nextTick();
    expect(host.querySelector('[role="progressbar"]')?.hasAttribute('aria-valuenow')).toBe(false);
  });
});
