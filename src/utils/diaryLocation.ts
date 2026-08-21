import {
  checkPermissions,
  getCurrentPosition,
  requestPermissions,
  type PermissionStatus,
  type Position,
} from '@tauri-apps/plugin-geolocation';
import type {DiaryLocation} from '../bindings';
import api from './api';

const LOCATION_REQUEST_TIMEOUT_MS = 20_000;

export type DiaryLocationErrorCode =
  | 'permissionDenied'
  | 'servicesDisabled'
  | 'timeout'
  | 'unavailable';

export class DiaryLocationError extends Error {
  constructor(
    public readonly code: DiaryLocationErrorCode,
    message: string,
  ) {
    super(message);
    this.name = 'DiaryLocationError';
  }
}

interface LocationDependencies {
  checkPermissions: typeof checkPermissions;
  requestPermissions: typeof requestPermissions;
  getCurrentPosition: typeof getCurrentPosition;
  reverseGeocode: (latitude: number, longitude: number) => Promise<string | null>;
}

const defaultDependencies: LocationDependencies = {
  checkPermissions,
  requestPermissions,
  getCurrentPosition,
  reverseGeocode: (latitude, longitude) => api.cmdReverseGeocode(latitude, longitude),
};

function permissionGranted(status: PermissionStatus): boolean {
  return status.location === 'granted' || status.coarseLocation === 'granted';
}

function permissionCanBeRequested(status: PermissionStatus): boolean {
  return status.location === 'prompt'
    || status.location === 'prompt-with-rationale'
    || status.coarseLocation === 'prompt'
    || status.coarseLocation === 'prompt-with-rationale';
}

function classifyLocationFailure(error: unknown): DiaryLocationError {
  if (error instanceof DiaryLocationError) return error;
  const message = error instanceof Error ? error.message : String(error);
  if (/location services are disabled/i.test(message)) {
    return new DiaryLocationError('servicesDisabled', '系统定位服务尚未开启');
  }
  return new DiaryLocationError('unavailable', message || '暂时无法获取当前位置');
}

async function withTimeout<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new DiaryLocationError(
      'timeout',
      '获取当前位置超时，请移到开阔位置后重试',
    )), timeoutMs);
  });
  try {
    return await Promise.race([promise, timeout]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

function finiteOptional(value: number | null): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

export function positionToDiaryLocation(position: Position): DiaryLocation {
  const {latitude, longitude, accuracy, altitude, altitudeAccuracy} = position.coords;
  if (!Number.isFinite(latitude) || latitude < -90 || latitude > 90
    || !Number.isFinite(longitude) || longitude < -180 || longitude > 180
    || !Number.isFinite(position.timestamp) || position.timestamp < 0) {
    throw new DiaryLocationError('unavailable', '系统返回了无效的位置数据');
  }

  const horizontalAccuracy = finiteOptional(accuracy);
  const verticalAccuracy = finiteOptional(altitudeAccuracy);
  // Android 插件没有暴露 Location.hasAltitude()；只有存在有效垂直精度时才保存高度，
  // 避免把系统用于“不可用”的默认 0 米误认为真实海拔。
  const validAltitude = verticalAccuracy !== null && verticalAccuracy >= 0
    ? finiteOptional(altitude)
    : null;

  return {
    coordinateSystem: 'wgs84',
    latitude,
    longitude,
    horizontalAccuracyMeters: horizontalAccuracy !== null && horizontalAccuracy >= 0
      ? horizontalAccuracy
      : null,
    capturedAt: position.timestamp,
    placeName: null,
    altitudeMeters: validAltitude,
    verticalAccuracyMeters: verticalAccuracy !== null && verticalAccuracy >= 0
      ? verticalAccuracy
      : null,
  };
}

export async function captureCurrentDiaryLocation(
  dependencies: LocationDependencies = defaultDependencies,
  timeoutMs = LOCATION_REQUEST_TIMEOUT_MS,
): Promise<DiaryLocation> {
  try {
    let permissions = await dependencies.checkPermissions();
    if (!permissionGranted(permissions) && permissionCanBeRequested(permissions)) {
      permissions = await dependencies.requestPermissions(['location']);
    }
    if (!permissionGranted(permissions)) {
      throw new DiaryLocationError(
        'permissionDenied',
        '未获得位置权限，请在系统设置中允许访问位置',
      );
    }

    const position = await withTimeout(dependencies.getCurrentPosition({
      enableHighAccuracy: true,
      maximumAge: 30_000,
      timeout: 15_000,
    }), timeoutMs);
    const location = positionToDiaryLocation(position);

    // Android Geocoder 只是尽力而为；地点名称失败不能阻止保存可靠的坐标。
    try {
      const placeName = (await dependencies.reverseGeocode(
        location.latitude,
        location.longitude,
      ))?.trim();
      location.placeName = placeName || null;
    } catch {
      location.placeName = null;
    }
    return location;
  } catch (error) {
    throw classifyLocationFailure(error);
  }
}

export function buildAmapLocationUrl(location: DiaryLocation): string {
  const url = new URL('https://uri.amap.com/marker');
  url.searchParams.set('position', `${location.longitude},${location.latitude}`);
  if (location.placeName?.trim()) url.searchParams.set('name', location.placeName.trim());
  url.searchParams.set('coordinate', 'wgs84');
  url.searchParams.set('callnative', '1');
  return url.toString();
}

export function formatLocationCoordinates(location: DiaryLocation): string {
  return `${location.latitude.toFixed(6)}, ${location.longitude.toFixed(6)}`;
}
