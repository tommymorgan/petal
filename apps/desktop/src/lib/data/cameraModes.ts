import { invoke } from '@tauri-apps/api/core';
import { session } from '$lib/stores/session.svelte';
import { COMMANDS, hasTauriBridge } from '$lib/ipc';
import type { AppliedCameraDevice, CameraMode } from '$lib/ipc';

export { hasTauriBridge } from '$lib/ipc';
export type { AppliedCameraDevice, CameraMode } from '$lib/ipc';

/** The resolution presets offered in Settings (16:9 / 4:3 fixed sizes). */
export interface CameraResolutionPreset {
  id: string;
  width: number;
  height: number;
  label: string;
}

export const CAMERA_RESOLUTION_PRESETS: CameraResolutionPreset[] = [
  { id: 'auto', width: 0, height: 0, label: 'Auto (best)' },
  { id: '480p', width: 640, height: 480, label: '480p' },
  { id: '720p', width: 1280, height: 720, label: '720p' },
  { id: '1080p', width: 1920, height: 1080, label: '1080p' },
  { id: '4k', width: 3840, height: 2160, label: '4K (2160p)' }
];

/** The FPS presets offered in Settings. */
export const CAMERA_FPS_PRESETS: number[] = [15, 30, 60];

/** Preserve the advertised ratio; display formatting must not alter the value. */
export function cameraModeFps(mode: CameraMode): number {
  if (mode.frameRateDenominator <= 0) return 0;
  return mode.frameRateNumerator / mode.frameRateDenominator;
}

/** Real device sizes, with Auto retained independently of enumeration. */
export function cameraResolutionOptions(modes: CameraMode[]): CameraResolutionPreset[] {
  const sizes = new Map<string, CameraResolutionPreset>();
  for (const mode of modes) {
    const fps = cameraModeFps(mode);
    if (mode.width <= 0 || mode.height <= 0 || !Number.isFinite(fps) || fps <= 0) continue;
    const id = `${mode.width}x${mode.height}`;
    sizes.set(id, {
      id,
      width: mode.width,
      height: mode.height,
      label: `${mode.width} × ${mode.height}`
    });
  }
  return [
    CAMERA_RESOLUTION_PRESETS[0],
    ...Array.from(sizes.values()).sort(
      (a, b) => a.width * a.height - b.width * b.height || a.width - b.width
    )
  ];
}

export function cameraFrameRateOptions(
  modes: CameraMode[],
  width: number,
  height: number
): number[] {
  return [...new Set(
    modes
      .filter((mode) => mode.width === width && mode.height === height)
      .map(cameraModeFps)
      .filter((fps) => Number.isFinite(fps) && fps > 0)
  )].sort((a, b) => b - a);
}

/** Whether any enumerated mode matches `width`x`height`. */
export function cameraSupportsResolution(
  modes: CameraMode[],
  width: number,
  height: number
): boolean {
  return modes.some((mode) => mode.width === width && mode.height === height);
}

/** Windows preset matching retains its existing tenth-then-integer rounding. */
export function cameraSupportsFps(
  modes: CameraMode[],
  width: number,
  height: number,
  fps: number
): boolean {
  return modes.some(
    (mode) =>
      mode.width === width &&
      mode.height === height &&
      Math.round(Math.round(cameraModeFps(mode) * 10) / 10) === fps
  );
}

export async function listCameraModes(
  deviceId?: string
): Promise<CameraMode[] | null> {
  if (!hasTauriBridge()) return null;
  return await invoke<CameraMode[]>(COMMANDS.listCameraModes, {
    preferredDeviceId: deviceId ?? null
  });
}

export async function setCameraPrefs(
  width: number | null,
  height: number | null,
  frameRate: number | null
): Promise<AppliedCameraDevice | null> {
  if (!hasTauriBridge()) return null;
  return await invoke<AppliedCameraDevice>(COMMANDS.setCameraPrefs, {
    width,
    height,
    frameRate
  });
}

export async function seedCameraModePreference(): Promise<void> {
  if (!hasTauriBridge() || !session.cameraMode) return;
  try {
    await setCameraPrefs(
      session.cameraMode.width,
      session.cameraMode.height,
      session.cameraMode.frameRate
    );
  } catch (e) {
    console.warn('seedCameraModePreference failed', e);
  }
}
