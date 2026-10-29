import { invoke } from '@tauri-apps/api/core';
import { session, updateAudioDevices, updateCameraMode } from '$lib/stores/session.svelte';
import { COMMANDS, hasTauriBridge } from '$lib/ipc';
import type { AppliedCameraDevice, CameraDeviceInfo } from '$lib/ipc';
import { isLinux, isMac } from '$lib/platform';
import { CAMERA_FPS_PRESETS, cameraFrameRateOptions, cameraSupportsFps, listCameraModes } from './cameraModes';

let selectionRequest = 0;

export { hasTauriBridge } from '$lib/ipc';
export type { AppliedCameraDevice, CameraDeviceInfo } from '$lib/ipc';

export async function listCameraDevices(): Promise<CameraDeviceInfo[] | null> {
  if (!hasTauriBridge()) return null;
  return await invoke<CameraDeviceInfo[]>(COMMANDS.listCameraDevices);
}

export async function setCameraDevice(deviceId: string): Promise<AppliedCameraDevice | null> {
  if (!hasTauriBridge()) return null;
  const request = ++selectionRequest;
  let mode = session.cameraMode;
  if (!isMac()) {
    const modes = (await listCameraModes(deviceId)) ?? [];
    if (request !== selectionRequest) return null;
    // A persisted mode belongs to the previous device. Keep its size if the
    // new device advertises it, but never send an old cadence to a new camera.
    // Explicit mode changes still use set_camera_prefs without this coercion.
    if (mode && deviceId !== session.cameraDeviceId) {
      const { width, height } = mode;
      const rates = isLinux()
        ? cameraFrameRateOptions(modes, width, height)
        : CAMERA_FPS_PRESETS.filter((fps) => cameraSupportsFps(modes, width, height, fps));
      mode = rates.length === 0 ? null : {
        ...mode,
        frameRate: rates.includes(mode.frameRate) ? mode.frameRate : rates[0]
      };
    }
  }
  updateAudioDevices(undefined, undefined, deviceId);
  updateCameraMode(mode);
  const applied = await invoke<AppliedCameraDevice>(COMMANDS.setCameraDevice, {
    deviceId,
    width: mode?.width ?? null,
    height: mode?.height ?? null,
    frameRate: mode?.frameRate ?? null
  });
  return request === selectionRequest ? applied : null;
}

export async function seedCameraDevicePreference(): Promise<void> {
  if (!hasTauriBridge() || !session.cameraDeviceId) return;
  try {
    // Startup seeds the saved pair together without a mode query or a
    // user-selection normalization. The initial grant may not exist yet.
    const mode = session.cameraMode;
    await invoke<AppliedCameraDevice>(COMMANDS.setCameraDevice, {
      deviceId: session.cameraDeviceId,
      width: mode?.width ?? null,
      height: mode?.height ?? null,
      frameRate: mode?.frameRate ?? null
    });
  } catch (e) {
    console.warn('seedCameraDevicePreference failed', e);
  }
}
