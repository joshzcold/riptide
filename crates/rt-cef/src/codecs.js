// Tells the page the codecs in `content.media.blocked_codecs` can't be
// played, so sites such as YouTube pick another one.
(function (blocked) {
  const PREFIXES = { av1: ["av01"], vp9: ["vp9", "vp09"], vp8: ["vp8"] };
  const prefixes = blocked.flatMap((c) => PREFIXES[c] || []);
  const isBlocked = (type) => {
    const codecs = /codecs\s*=\s*"?([^";]*)/i.exec(String(type));
    return !!codecs && codecs[1].split(",").some((c) => {
      const codec = c.trim().toLowerCase();
      return prefixes.some((p) => codec.startsWith(p));
    });
  };
  const wrap = (owner, name, unsupported) => {
    const original = owner && owner[name];
    if (typeof original !== "function") return;
    owner[name] = {
      [name](config) {
        const type = typeof config === "string" ? config : config?.video?.contentType;
        return isBlocked(type) ? unsupported() : original.apply(this, arguments);
      },
    }[name];
  };
  wrap(window.MediaSource, "isTypeSupported", () => false);
  wrap(window.ManagedMediaSource, "isTypeSupported", () => false);
  wrap(HTMLMediaElement.prototype, "canPlayType", () => "");
  wrap(window.MediaCapabilities?.prototype, "decodingInfo",
    () => Promise.resolve({ supported: false, smooth: false, powerEfficient: false }));
})
