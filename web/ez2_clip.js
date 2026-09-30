// Reading the frames of a video in the browser, for importing it as an
// animation (the web build has no ffmpeg). The browser plays the video in
// a hidden <video>; each frame is drawn at its time onto a canvas.
"use strict";

(function () {
  function once(el, ok, fail) {
    return new Promise((resolve, reject) => {
      const done = (f, v) => () => {
        el.removeEventListener(ok, onOk);
        if (fail) el.removeEventListener(fail, onFail);
        f(v);
      };
      const onOk = done(resolve);
      const onFail = done(reject, new Error("this video can't be played in this browser"));
      el.addEventListener(ok, onOk);
      if (fail) el.addEventListener(fail, onFail);
    });
  }

  async function seek(video, t) {
    const seeked = once(video, "seeked");
    video.currentTime = t;
    await seeked;
  }

  // Frames of `bytes` (a video file) at most `maxSide` pixels on their
  // longer side, at most `maxFps` a second and `maxFrames` in all, from
  // the first `maxSeconds`. Resolves to {width, height, count, seconds,
  // pixels}: `count` RGBA frames one after the other.
  window.ez2DecodeVideo = async function (bytes, maxSide, maxFrames, maxFps, maxSeconds) {
    const url = URL.createObjectURL(new Blob([bytes]));
    const video = document.createElement("video");
    video.muted = true;
    video.playsInline = true;
    video.preload = "auto";
    try {
      const loaded = once(video, "loadeddata", "error");
      video.src = url;
      await loaded;
      let duration = video.duration;
      if (!isFinite(duration)) {
        // Recorded WebM files often don't say how long they are until
        // the end has been seen.
        await seek(video, 1e7);
        duration = video.duration;
      }
      if (!isFinite(duration) || duration <= 0) duration = maxSeconds;
      const length = Math.min(duration, maxSeconds);
      const fps = Math.min(maxFps, maxFrames / length);
      const count = Math.max(1, Math.floor(length * fps));
      const vw = video.videoWidth, vh = video.videoHeight;
      if (!vw || !vh) throw new Error("no picture in this video");
      const k = Math.min(1, maxSide / Math.max(vw, vh));
      const w = Math.max(2, Math.round((vw * k) / 2) * 2);
      const h = Math.max(2, Math.round((vh * k) / 2) * 2);
      const canvas = document.createElement("canvas");
      canvas.width = w;
      canvas.height = h;
      const g = canvas.getContext("2d", { willReadFrequently: true });
      const pixels = new Uint8Array(w * h * 4 * count);
      for (let i = 0; i < count; i++) {
        await seek(video, Math.min((i + 0.5) / fps, duration - 1e-3));
        g.drawImage(video, 0, 0, w, h);
        pixels.set(g.getImageData(0, 0, w, h).data, i * w * h * 4);
      }
      return { width: w, height: h, count, seconds: count / fps, pixels };
    } finally {
      video.removeAttribute("src");
      video.load();
      URL.revokeObjectURL(url);
    }
  };
})();
