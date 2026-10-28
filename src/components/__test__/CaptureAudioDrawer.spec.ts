// @vitest-environment happy-dom
import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {createApp, h, nextTick, type App} from 'vue';
import CaptureAudioDrawer from '../CaptureAudioDrawer.vue';

const mocks = vi.hoisted(() => ({waveform: vi.fn(), fixDuration: vi.fn(), notify: vi.fn()}));
vi.mock('quasar', () => ({useQuasar: () => ({notify: mocks.notify})}));
vi.mock('../../utils/setWebmDuration', () => ({setWebmDuration: mocks.fixDuration}));
vi.mock('../../utils/audioWaveform', () => ({AUDIO_WAVEFORM_VERSION: 1, generateAudioWaveform: mocks.waveform}));

let app: App | undefined;
let getMedia: ReturnType<typeof vi.fn>;
let trackStop: ReturnType<typeof vi.fn>;
let contextClose: ReturnType<typeof vi.fn>;
let saved: ReturnType<typeof vi.fn<(...args: unknown[]) => void>>;
let closed: ReturnType<typeof vi.fn<() => void>>;
let mimeType: string;
let stopFailure = false;
let previewFailure = false;
let recorders: FakeRecorder[];
const generated = {durationMs: 1234, waveform: {version: 1, peaks: [128, 192]}};

class FakeRecorder extends EventTarget {
    static isTypeSupported(type: string) { return type === mimeType; }
    state: RecordingState = 'inactive';
    mimeType = mimeType;
    ondataavailable: ((event: BlobEvent) => void) | null = null;
    start = vi.fn(() => { this.state = 'recording'; });
    requestData = vi.fn();
    stop = vi.fn(() => {
        if (stopFailure) throw new Error('设备临时失败');
        this.state = 'inactive';
        queueMicrotask(() => {
            this.ondataavailable?.({data: new Blob([new Uint8Array([1, 2, 3])], {type: mimeType})} as BlobEvent);
            this.dispatchEvent(new Event('stop'));
        });
    });
    constructor() { super(); recorders.push(this); }
}
async function settle() {
    await vi.advanceTimersByTimeAsync(0);
    await nextTick();
}
function button(label: string) {
    return document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
}
async function mount() {
    const host = document.createElement('div'); document.body.append(host);
    app = createApp({render: () => h(CaptureAudioDrawer, {
        visible: true, onRecorded: (...args: unknown[]) => saved(...args), onClose: () => closed(),
    })});
    app.mount(host); await settle();
}
async function start() { button('开始录音').click(); await settle(); }
async function stop() { button('停止录音').click(); await settle(); }

beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal('MediaRecorder', FakeRecorder);
    recorders = []; mimeType = 'audio/webm;codecs=opus'; stopFailure = false; previewFailure = false;
    trackStop = vi.fn(); contextClose = vi.fn(async () => undefined);
    getMedia = vi.fn(async () => ({getTracks: () => [{stop: trackStop}]}));
    Object.defineProperty(navigator, 'mediaDevices', {value: {getUserMedia: getMedia}, configurable: true});
    vi.stubGlobal('AudioContext', class {
        state = 'running'; close = contextClose;
        constructor() { if (previewFailure) throw new Error('音波设备不可用'); }
        createMediaStreamSource() { return {connect: vi.fn(), disconnect: vi.fn()}; }
        createAnalyser() { return {disconnect: vi.fn(), fftSize: 256}; }
    });
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null);
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    mocks.fixDuration.mockReset().mockImplementation(buffer => buffer);
    mocks.waveform.mockReset().mockResolvedValue(generated);
    mocks.notify.mockClear(); saved = vi.fn(); closed = vi.fn();
});
afterEach(() => {
    app?.unmount(); app = undefined; document.body.innerHTML = '';
    vi.restoreAllMocks(); vi.unstubAllGlobals(); vi.useRealTimers();
});

describe('recording lifecycle', () => {
    it('stops the microphone, includes final data, saves once and closes', async () => {
        await mount(); await start(); await vi.advanceTimersByTimeAsync(1300); await stop();
        expect(trackStop).toHaveBeenCalledTimes(1);
        expect(contextClose).toHaveBeenCalledTimes(1);
        expect(saved).toHaveBeenCalledExactlyOnceWith(mimeType, new Uint8Array([1, 2, 3]), generated);
        expect(closed).toHaveBeenCalledTimes(1);
        expect(mocks.fixDuration.mock.calls[0][1]).toBe(1300);
    });
    it('falls back to the original WebM when duration repair fails and can record again', async () => {
        mocks.fixDuration.mockImplementation(() => { throw new Error("cant' find info element"); });
        await mount(); await start(); await stop();
        expect(saved).toHaveBeenCalledTimes(1);
        expect(saved.mock.calls[0][1]).toEqual(new Uint8Array([1, 2, 3]));
        await start(); await stop(); expect(saved).toHaveBeenCalledTimes(2);
        expect(recorders).toHaveLength(2);
    });
    it('does not parse MP4 as WebM', async () => {
        mimeType = 'audio/mp4'; await mount(); await start(); await stop();
        expect(mocks.fixDuration).not.toHaveBeenCalled(); expect(saved).toHaveBeenCalledTimes(1);
    });
    it('still saves duration and an empty waveform when decoding fails', async () => {
        mocks.waveform.mockRejectedValue(new Error('解码失败'));
        await mount(); await start(); await vi.advanceTimersByTimeAsync(900); await stop();
        expect(saved.mock.calls[0][2]).toEqual({durationMs: 900, waveform: {version: 1, peaks: []}});
        expect(closed).toHaveBeenCalledTimes(1);
    });
    it('shows processing, prevents repeated starts/closes, and saves after a decode timeout', async () => {
        mocks.waveform.mockReturnValue(new Promise(() => undefined));
        await mount(); await start(); await stop();
        expect(button('正在保存录音').disabled).toBe(true);
        expect(button('停止并关闭录音').disabled).toBe(true);
        button('正在保存录音').click(); button('停止并关闭录音').click();
        expect(getMedia).toHaveBeenCalledTimes(1); expect(saved).not.toHaveBeenCalled();
        await vi.advanceTimersByTimeAsync(5000); await settle();
        expect(saved).toHaveBeenCalledTimes(1); expect(closed).toHaveBeenCalledTimes(1);
        expect(recorders[0].stop).toHaveBeenCalledTimes(1);
    });
    it('allows retry after stop throws without trapping the stopping flag', async () => {
        await mount(); await start(); stopFailure = true; await stop();
        expect(document.querySelector('[role="alert"]')?.textContent).toContain('设备临时失败');
        expect(saved).not.toHaveBeenCalled(); stopFailure = false;
        button('重试保存录音').click(); await settle();
        expect(saved).toHaveBeenCalledTimes(1); expect(trackStop).toHaveBeenCalledTimes(1);
    });
    it('keeps the stopped recording available when reading its bytes fails', async () => {
        vi.spyOn(Blob.prototype, 'arrayBuffer').mockRejectedValueOnce(new Error('读取失败'));
        await mount(); await start(); await stop();
        expect(saved).not.toHaveBeenCalled(); expect(button('重试保存录音').disabled).toBe(false);
        button('重试保存录音').click(); await settle();
        expect(saved).toHaveBeenCalledTimes(1); expect(recorders[0].stop).toHaveBeenCalledTimes(1);
        expect(getMedia).toHaveBeenCalledTimes(1);
    });
    it('can stop and save even if realtime waveform initialization fails', async () => {
        previewFailure = true; await mount(); await start(); await stop();
        expect(saved).toHaveBeenCalledTimes(1); expect(trackStop).toHaveBeenCalledTimes(1);
    });
    it('guards repeated clicks while microphone permission is pending', async () => {
        let resolve!: (stream: {getTracks: () => {stop: typeof trackStop}[]}) => void;
        getMedia.mockReturnValue(new Promise(r => { resolve = r; }));
        await mount(); button('开始录音').click(); await settle(); button('开始录音').click();
        button('停止并关闭录音').click();
        expect(getMedia).toHaveBeenCalledTimes(1); expect(closed).not.toHaveBeenCalled();
        resolve({getTracks: () => [{stop: trackStop}]}); await settle(); await stop();
        expect(saved).toHaveBeenCalledTimes(1);
    });
});
