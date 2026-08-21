import {describe, expect, it, vi} from 'vitest';
import type {PermissionStatus, Position} from '@tauri-apps/plugin-geolocation';
import {
  buildAmapLocationUrl,
  captureCurrentDiaryLocation,
  DiaryLocationError,
  positionToDiaryLocation,
} from '../diaryLocation';

const granted: PermissionStatus = {location: 'granted', coarseLocation: 'granted'};
const position: Position = {
  timestamp: 1_787_392_800_000,
  coords: {
    latitude: 23.1291,
    longitude: 113.2644,
    accuracy: 18.5,
    altitude: 12.3,
    altitudeAccuracy: 4.5,
    speed: null,
    heading: null,
  },
};

function dependencies(permissionStatus = granted) {
  return {
    checkPermissions: vi.fn().mockResolvedValue(permissionStatus),
    requestPermissions: vi.fn().mockResolvedValue(granted),
    getCurrentPosition: vi.fn().mockResolvedValue(position),
    reverseGeocode: vi.fn().mockResolvedValue(' 广州市越秀区 '),
  };
}

describe('Android diary location capture', () => {
  it('captures WGS84 coordinates and an optional system place name', async () => {
    const deps = dependencies();

    const result = await captureCurrentDiaryLocation(deps);

    expect(deps.requestPermissions).not.toHaveBeenCalled();
    expect(deps.getCurrentPosition).toHaveBeenCalledWith({
      enableHighAccuracy: true,
      maximumAge: 30_000,
      timeout: 15_000,
    });
    expect(result).toEqual({
      coordinateSystem: 'wgs84',
      latitude: 23.1291,
      longitude: 113.2644,
      horizontalAccuracyMeters: 18.5,
      capturedAt: 1_787_392_800_000,
      placeName: '广州市越秀区',
      altitudeMeters: 12.3,
      verticalAccuracyMeters: 4.5,
    });
  });

  it('accepts Android approximate location permission', async () => {
    const deps = dependencies({location: 'denied', coarseLocation: 'prompt'});
    deps.requestPermissions.mockResolvedValue({
      location: 'denied',
      coarseLocation: 'granted',
    });

    await expect(captureCurrentDiaryLocation(deps)).resolves.toMatchObject({
      latitude: 23.1291,
      horizontalAccuracyMeters: 18.5,
    });
    expect(deps.requestPermissions).toHaveBeenCalledWith(['location']);
  });

  it('reports denied permission before requesting a position', async () => {
    const deps = dependencies({location: 'denied', coarseLocation: 'denied'});

    await expect(captureCurrentDiaryLocation(deps)).rejects.toMatchObject({
      code: 'permissionDenied',
    } satisfies Partial<DiaryLocationError>);
    expect(deps.getCurrentPosition).not.toHaveBeenCalled();
  });

  it('keeps coordinates when reverse geocoding is unavailable', async () => {
    const deps = dependencies();
    deps.reverseGeocode.mockRejectedValue(new Error('network unavailable'));

    await expect(captureCurrentDiaryLocation(deps)).resolves.toMatchObject({
      latitude: 23.1291,
      placeName: null,
    });
  });

  it('rejects malformed native positions', () => {
    expect(() => positionToDiaryLocation({
      ...position,
      coords: {...position.coords, latitude: 91},
    })).toThrow('无效的位置数据');
  });

  it('marks the original coordinate system when opening Amap', () => {
    const location = positionToDiaryLocation(position);
    location.placeName = '广州市越秀区';
    const url = new URL(buildAmapLocationUrl(location));

    expect(url.origin + url.pathname).toBe('https://uri.amap.com/marker');
    expect(url.searchParams.get('position')).toBe('113.2644,23.1291');
    expect(url.searchParams.get('coordinate')).toBe('wgs84');
    expect(url.searchParams.get('callnative')).toBe('1');
    expect(url.searchParams.get('name')).toBe('广州市越秀区');
  });
});
