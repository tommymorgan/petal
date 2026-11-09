// Native camera → webview self-view feed, JS consumer.
//
// The Windows and Linux self-view draws the native publication capture's
// frames, pulled directly from the Rust process (`next_self_view_frame` →
// raw ArrayBuffer; layout documented in src-tauri/src/camera_self_view.rs)
// onto an offscreen source canvas — no SFU round trip, no separate native
// window, and the camera light is on exactly once (one capture client).
//
// Windows (WebView2) turns that canvas into a MediaStream with
// `captureStream(60)` so the existing `<video>` surfaces play it. The Linux
// WebKitGTK build has no usable canvas capture stream (the `<video>` never
// leaves readyState 0, and the GStreamer GL sink crashed the web process on
// NVIDIA), so Linux hands out a canvas feed instead: each surface mirrors
// every new frame onto its own visible `<canvas>`.
//
// The captureStream rate is a CAP, not a target: the canvas only paints when a
// genuinely new frame arrives, so the stream never fabricates frames beyond
// what the camera actually delivers. The pull loop overlaps the IPC round
// trip with the next vsync rather than awaiting it inside the rAF tick.
//
// WebCodecs `VideoFrame` (NV12) is available in evergreen WebView2
// (Chromium ≥ 94) and in the WebKitGTK the Linux build runs.

import { invoke } from '@tauri-apps/api/core';
import { COMMANDS } from '$lib/ipc';
import type { CanvasCameraFeed } from '$lib/videoAttachment';

/** `[width: u32][height: u32][capture_wall_time_us: u64]` little-endian. */
const SELF_VIEW_HEADER_BYTES = 16;

/** Self-view stream cap — the highest fps preset the UI can request. The
 * canvas only paints when a new frame arrives, so this is a ceiling, never a
 * frame factory: a 30fps capture yields a 30fps stream under this cap. */
const SELF_VIEW_MAX_FPS = 60;

let stream: MediaStream | null = null;
let cancelTick: (() => void) | null = null;
let cancelled = false;
let pulling = false;
const frameListeners = new Set<() => void>();

/** Linux pull cadence. WebKitGTK withholds requestAnimationFrame while
 * GNOME is not presenting the window (observed: zero ticks for a mapped
 * meeting window), so its loop runs on a timer instead of vsync. */
const TIMER_PULL_INTERVAL_MS = 1000 / SELF_VIEW_MAX_FPS;

/** Start the shared pull loop drawing onto a fresh source canvas. `onDraw`
 * runs after every genuinely new frame; `useTimer` swaps the vsync cadence
 * for a fixed timer. Any prior loop is stopped first, so start/stop can
 * never stack two pull loops. */
function startPullLoop(onDraw: () => void, useTimer = false): HTMLCanvasElement {
  stopNativeSelfView();
  cancelled = false;

  const canvas = document.createElement('canvas');
  canvas.width = 640;
  canvas.height = 360;
  const ctx = canvas.getContext('2d');
  if (!ctx) {
    stopNativeSelfView();
    throw new Error('2d canvas context unavailable');
  }
  let lastDrawnTimestamp = -1;

  const loop = () => {
    if (cancelled) return;
    if (!pulling) {
      pulling = true;
      invoke<ArrayBuffer>(COMMANDS.nextSelfViewFrame)
        .then((buf) => {
          if (cancelled || buf.byteLength === 0) return;
          const view = new DataView(buf);
          const width = view.getUint32(0, true);
          const height = view.getUint32(4, true);
          const timestamp = Number(view.getBigUint64(8, true));
          if (timestamp === lastDrawnTimestamp) return;
          lastDrawnTimestamp = timestamp;
          // A zero-copy view that starts at the Y plane, with the default
          // tightly packed NV12 layout. WebKitGTK ignores explicit `layout`
          // plane offsets: offset-based framing skipped no header there,
          // shifting luma 16px and misaligning chroma into blotchy
          // gray/colored patches (measured mean error 110 vs 7 per pixel).
          const frame = new VideoFrame(new Uint8Array(buf, SELF_VIEW_HEADER_BYTES), {
            format: 'NV12',
            codedWidth: width,
            codedHeight: height,
            timestamp
          });
          if (canvas.width !== width || canvas.height !== height) {
            canvas.width = width;
            canvas.height = height;
          }
          ctx.drawImage(frame, 0, 0);
          frame.close();
          onDraw();
        })
        .catch((error) => {
          console.error('native self-view: frame pull failed', error);
        })
        .finally(() => {
          pulling = false;
        });
    }
    schedule();
  };
  const schedule = () => {
    if (useTimer) {
      const id = setTimeout(loop, TIMER_PULL_INTERVAL_MS);
      cancelTick = () => clearTimeout(id);
    } else {
      const id = requestAnimationFrame(loop);
      cancelTick = () => cancelAnimationFrame(id);
    }
  };
  schedule();
  return canvas;
}

/** Windows: the native-fed self-view as a MediaStream for `<video>`. */
export async function startNativeSelfView(): Promise<MediaStream> {
  const canvas = startPullLoop(() => {});
  stream = canvas.captureStream(SELF_VIEW_MAX_FPS);
  return stream;
}

/** Linux: the native-fed self-view as a canvas feed. Each attached surface
 * receives a copy of every new frame on its own visible canvas. */
export async function startNativeSelfViewFeed(): Promise<CanvasCameraFeed> {
  const source = startPullLoop(() => {
    for (const listener of frameListeners) listener();
  }, true);
  return {
    kind: 'canvas',
    attach(target, onFrame) {
      const ctx = target.getContext('2d');
      if (!ctx) return () => {};
      const draw = () => {
        if (target.width !== source.width || target.height !== source.height) {
          target.width = source.width;
          target.height = source.height;
        }
        ctx.drawImage(source, 0, 0);
        onFrame?.();
      };
      frameListeners.add(draw);
      return () => {
        frameListeners.delete(draw);
      };
    }
  };
}

/** Stop the native-fed self-view: cancel the pull loop, stop the canvas
 * stream's tracks and detach every canvas surface. Idempotent. */
export function stopNativeSelfView(): void {
  cancelled = true;
  cancelTick?.();
  cancelTick = null;
  stream?.getTracks().forEach((track) => track.stop());
  stream = null;
  frameListeners.clear();
}
