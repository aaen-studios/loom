import { quantise, type Swatch } from "./palette";

/**
 * Reads the colours out of a background image or video.
 *
 * Browser-only, and deliberately small: the picture is downscaled to a couple of
 * hundred pixels a side before sampling. Detail is irrelevant — what is being
 * measured is which colours the picture is *made of* — and a full-resolution
 * `getImageData` of a 4K wallpaper is tens of megabytes of work for an answer
 * that is identical at 256px.
 */

/** Longest edge of the downscaled sample. Colour distribution does not need
 *  more, and this keeps the read to well under a megabyte. */
const SAMPLE_EDGE = 256;

/** Where a video is sampled, as a fraction of its duration.
 *
 * Not frame zero, which is frequently a title card or a fade from black, and
 * not the middle, which on a looping animation can land on a transition. A
 * quarter in is past any lead-in on almost everything.
 */
const VIDEO_SEEK = 0.25;

/** How long to wait for a video to produce a frame before giving up. */
const VIDEO_TIMEOUT_MS = 4000;

async function swatchesFrom(source: CanvasImageSource, width: number, height: number): Promise<Swatch[]> {
  if (width === 0 || height === 0) return [];

  const scale = Math.min(1, SAMPLE_EDGE / Math.max(width, height));
  const w = Math.max(1, Math.round(width * scale));
  const h = Math.max(1, Math.round(height * scale));

  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) return [];

  context.drawImage(source, 0, 0, w, h);
  const { data } = context.getImageData(0, 0, w, h);
  return quantise(data);
}

/**
 * The palette of a still image.
 *
 * Returns an empty list rather than throwing for anything that will not load.
 * An adaptive theme with no swatches falls back to the default accent, which is
 * a usable theme — so a broken file degrades to "the theme did not adapt"
 * rather than to "the app has no colours".
 */
export async function sampleImage(url: string): Promise<Swatch[]> {
  if (typeof document === "undefined") return [];
  const image = new Image();
  // Required before `drawImage`: a canvas tainted by a cross-origin image
  // makes `getImageData` throw a security error. Loom's own backgrounds are
  // same-origin `asset:` URLs, but this keeps the failure honest if one is not.
  image.crossOrigin = "anonymous";

  try {
    await new Promise<void>((resolve, reject) => {
      image.onload = () => resolve();
      image.onerror = () => reject(new Error("could not load the image"));
      image.src = url;
    });
    return await swatchesFrom(image, image.naturalWidth, image.naturalHeight);
  } catch {
    return [];
  }
}

/**
 * The palette of a video, from one frame.
 *
 * A video cannot be drawn to a canvas until it has decoded a frame, so this
 * mounts a hidden element, seeks, and waits for `seeked`. The element is muted
 * and never played: sampling a background must not make a sound or advance a
 * loop the user is looking at.
 */
export async function sampleVideo(url: string): Promise<Swatch[]> {
  if (typeof document === "undefined") return [];
  const video = document.createElement("video");
  video.crossOrigin = "anonymous";
  video.muted = true;
  video.playsInline = true;
  video.preload = "metadata";

  try {
    await new Promise<void>((resolve, reject) => {
      const fail = () => reject(new Error("could not load the video"));
      video.onloadedmetadata = () => resolve();
      video.onerror = fail;
      video.src = url;
    });

    const duration = Number.isFinite(video.duration) ? video.duration : 0;
    const target = duration > 0 ? duration * VIDEO_SEEK : 0;

    await new Promise<void>((resolve) => {
      // A video that never emits `seeked` — a codec the webview cannot decode,
      // or a zero-length file — must not hang the theme. Resolving on the timer
      // instead means the sample is simply empty.
      const timer = window.setTimeout(resolve, VIDEO_TIMEOUT_MS);
      video.onseeked = () => {
        window.clearTimeout(timer);
        resolve();
      };
      // A zero-duration video is already at its (only) frame.
      if (target === 0) {
        window.clearTimeout(timer);
        resolve();
        return;
      }
      video.currentTime = target;
    });

    return await swatchesFrom(video, video.videoWidth, video.videoHeight);
  } catch {
    return [];
  } finally {
    // Releases the decoder and stops any further loading.
    video.removeAttribute("src");
    video.load();
  }
}

/**
 * Samples whatever kind of background the config points at.
 *
 * Cached by URL for the life of the process, because the answer cannot change
 * for a given file and the sampler is not free: switching between Adaptive and
 * Custom in Settings should not re-decode a 4K wallpaper each time.
 *
 * A sample that comes back empty is *not* kept. An empty list means the file
 * could not be read, which is exactly the case that should be retried — a
 * permanent entry would let one failed decode pin a path for the whole session,
 * and a file can fail for reasons that pass later (a slow read, or a video that
 * was still being written when it was first requested).
 */
const cache = new Map<string, Promise<Swatch[]>>();

export function sampleBackground(
  kind: "image" | "video",
  url: string,
): Promise<Swatch[]> {
  const cached = cache.get(url);
  if (cached) return cached;

  const pending = (kind === "video" ? sampleVideo(url) : sampleImage(url)).then(
    (found) => {
      // A failed sample must not stay cached. An empty list means "this file
      // could not be read", not "this file has no colours", and keeping it
      // would make one bad decode permanent for that path — so a file the user
      // has since replaced, or that failed only because the image element was
      // not ready, would reuse the failure for the rest of the session.
      if (found.length === 0) cache.delete(url);
      return found;
    },
  );
  cache.set(url, pending);
  return pending;
}

/** Drops a cached sample. Exposed for tests; nothing in the app needs it. */
export function clearSampleCache(): void {
  cache.clear();
}
