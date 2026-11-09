/** A self-view drawn frame by frame onto canvases rather than played as a
 * MediaStream (Linux WebKitGTK has no usable canvas capture stream). */
export interface CanvasCameraFeed {
  readonly kind: 'canvas';
  /** Copy every new frame onto `target`, calling `onFrame` after each, until
   * the returned detach function runs. */
  attach(target: HTMLCanvasElement, onFrame?: () => void): () => void;
}

/** What a camera surface renders: a playable stream or a canvas feed. */
export type CameraFeed = MediaStream | CanvasCameraFeed;

export function isCanvasFeed(feed: CameraFeed | null | undefined): feed is CanvasCameraFeed {
  return !!feed && 'kind' in feed && feed.kind === 'canvas';
}

export function attachVideoStream(
  videoEl: HTMLVideoElement | null,
  videoStream: MediaStream | null | undefined
): boolean {
  if (!videoEl) return false;

  const nextStream = videoStream ?? null;
  if (videoEl.srcObject === nextStream) return false;

  videoEl.srcObject = nextStream;
  return true;
}
