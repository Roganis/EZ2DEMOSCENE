// Saving files from EZ2DEMOSCENE.
// - In a browser: a normal download.
// - In the Android app (Capacitor WebView), downloads don't work: the file is
//   written to the app cache and handed to Android's share sheet, from where
//   it can be saved to Files/Drive or sent to another app.
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

  globalThis.ez2SaveFile = async function (name, bytes) {
    const cap = globalThis.Capacitor;
    if (cap && cap.isNativePlatform && cap.isNativePlatform() && cap.Plugins.Filesystem) {
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
