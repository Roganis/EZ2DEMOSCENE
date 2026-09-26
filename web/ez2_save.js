// Saving files from EZ2DEMOSCENE.
// - In a browser: a normal download.
// - In the Android app (Capacitor WebView), downloads don't work: Android's
//   "Save as…" dialog opens (the app's Ez2SaveAs plugin) so the file can be
//   saved anywhere in Files or Drive. Older app builds without the plugin
//   fall back to the share sheet.
"use strict";

(function () {
  function toBase64(bytes) {
    let s = "";
    const chunk = 0x8000;
    for (let i = 0; i < bytes.length; i += chunk) {
      s += String.fromCharCode.apply(null, bytes.subarray(i, i + chunk));
    }
    return btoa(s);
  }

  function browserDownload(name, bytes) {
    const url = URL.createObjectURL(new Blob([bytes]));
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 10000);
  }

  const MIME = {
    gif: "image/gif",
    png: "image/png",
    mp4: "video/mp4",
    webm: "video/webm",
    zip: "application/zip",
    json: "application/json",
  };

  function mimeFor(name) {
    const ext = name.split(".").pop().toLowerCase();
    // Unknown types (.ez2pack) as generic bytes: the picker keeps the name.
    return MIME[ext] || "application/octet-stream";
  }

  globalThis.ez2SaveFile = async function (name, bytes) {
    const cap = globalThis.Capacitor;
    const native = cap && cap.isNativePlatform && cap.isNativePlatform();
    if (native && cap.Plugins.Ez2SaveAs) {
      const res = await cap.Plugins.Ez2SaveAs.save({ name, data: toBase64(bytes), mime: mimeFor(name) });
      return res && res.saved;
    }
    if (native && cap.Plugins.Filesystem) {
      const { Filesystem, Share } = cap.Plugins;
      const res = await Filesystem.writeFile({ path: name, data: toBase64(bytes), directory: "CACHE" });
      if (Share) {
        await Share.share({ title: name, dialogTitle: "Save or share " + name, files: [res.uri] });
      }
      return;
    }
    browserDownload(name, bytes);
  };
})();
