// EZ2DEMOSCENE web video export: encodes RGBA frames with the browser's
// WebCodecs VideoEncoder (hardware accelerated where available) and muxes
// them into MP4 (mp4-muxer) or WebM (webm-muxer). Frame timestamps are exact,
// so exported loops stay seamless.
"use strict";

(function () {
  const MP4_CODECS = [
    // [WebCodecs codec string, mp4-muxer codec]
    ["avc1.640033", "avc"], // H.264 High 5.1 (up to 4K)
    ["avc1.640028", "avc"], // H.264 High 4.0 (1080p)
    ["avc1.4d0028", "avc"], // H.264 Main 4.0
    ["avc1.42e01f", "avc"], // H.264 Baseline 3.1 (720p)
    ["vp09.00.40.08", "vp9"],
    ["av01.0.08M.08", "av1"],
  ];
  const WEBM_CODECS = [
    ["vp09.00.40.08", "V_VP9"],
    ["vp8", "V_VP8"],
    ["av01.0.08M.08", "V_AV1"],
  ];

  function bitrateFor(w, h, fps) {
    // ~0.15 bits per pixel per frame: high quality for sharp demo visuals.
    return Math.round(Math.min(60e6, Math.max(2e6, w * h * fps * 0.15)));
  }

  async function pick(format, w, h, fps) {
    if (typeof VideoEncoder === "undefined") return null;
    const list = format === "mp4" ? MP4_CODECS : WEBM_CODECS;
    for (const [codec, muxCodec] of list) {
      const config = {
        codec,
        width: w,
        height: h,
        bitrate: bitrateFor(w, h, fps),
        framerate: fps,
      };
      if (codec.startsWith("avc1")) config.avc = { format: "avc" };
      try {
        const s = await VideoEncoder.isConfigSupported(config);
        if (s.supported) return { config, muxCodec };
      } catch (_) {
        /* try the next codec */
      }
    }
    return null;
  }

  class Ez2VideoExporter {
    // Resolves to an exporter, or rejects with a readable message.
    static async create(format, w, h, fps) {
      const choice = await pick(format, w, h, fps);
      if (!choice) {
        throw new Error(
          typeof VideoEncoder === "undefined"
            ? "This browser has no WebCodecs video encoder. Export a GIF or PNG frames instead."
            : "No " + format.toUpperCase() + " video codec is available for " + w + "×" + h + " in this browser."
        );
      }
      return new Ez2VideoExporter(format, w, h, fps, choice);
    }

    static available() {
      return typeof VideoEncoder !== "undefined";
    }

    constructor(format, w, h, fps, choice) {
      this.w = w;
      this.h = h;
      this.fps = fps;
      this.codec = choice.config.codec;
      this.error = null;
      if (format === "mp4") {
        this.muxer = new Mp4Muxer.Muxer({
          target: new Mp4Muxer.ArrayBufferTarget(),
          video: { codec: choice.muxCodec, width: w, height: h, frameRate: fps },
          fastStart: "in-memory",
        });
      } else {
        this.muxer = new WebMMuxer.Muxer({
          target: new WebMMuxer.ArrayBufferTarget(),
          video: { codec: choice.muxCodec, width: w, height: h, frameRate: fps },
        });
      }
      this.encoder = new VideoEncoder({
        output: (chunk, meta) => this.muxer.addVideoChunk(chunk, meta),
        error: (e) => {
          this.error = String(e);
        },
      });
      this.encoder.configure(choice.config);
    }

    // rgba: Uint8Array of w*h*4 bytes (sRGB).
    addFrame(rgba, index) {
      if (this.error) throw new Error(this.error);
      const frame = new VideoFrame(rgba, {
        format: "RGBA",
        codedWidth: this.w,
        codedHeight: this.h,
        timestamp: Math.round((index * 1e6) / this.fps),
        duration: Math.round(1e6 / this.fps),
      });
      this.encoder.encode(frame, { keyFrame: index % Math.max(1, Math.round(this.fps)) === 0 });
      frame.close();
    }

    queueSize() {
      return this.encoder.encodeQueueSize;
    }

    codecName() {
      return this.codec;
    }

    // Resolves to the finished file as a Uint8Array.
    async finish() {
      await this.encoder.flush();
      if (this.error) throw new Error(this.error);
      this.encoder.close();
      this.muxer.finalize();
      return new Uint8Array(this.muxer.target.buffer);
    }
  }

  globalThis.Ez2VideoExporter = Ez2VideoExporter;
})();
