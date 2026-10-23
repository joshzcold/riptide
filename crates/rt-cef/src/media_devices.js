// Steers a page's camera, microphone and speaker to the ones :media-devices
// picked for its site (`set`), switching them in a running call, and reports
// the page's devices to the browser (`list`). Devices are kept by name, since
// each site sees its own ids. `share` lets the next F23 key switch what the
// page shares (:share-switch).
(function (report) {
  const wanted = { camera: "", microphone: "", speaker: "" };
  const KIND = { camera: "videoinput", microphone: "audioinput", speaker: "audiooutput" };
  const streams = new Set();
  // Each getDisplayMedia stream, with what the site asked for.
  const shares = new Set();
  const connections = new Set();
  const outputs = new Set();
  // The site's track → the one riptide put in its place.
  const replaced = new WeakMap();
  let getUserMedia = null, displayMedia = null, elementSink = null, contextSink = null, observer = null;
  let shareArmed = 0;

  const idFor = async (kind) => {
    if (!wanted[kind] || !navigator.mediaDevices) return null;
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      return devices.find((d) => d.kind === KIND[kind] && d.label === wanted[kind])?.deviceId ?? null;
    } catch (e) {
      return null;
    }
  };
  const pin = (constraint, id) => ({ ...(typeof constraint === "object" ? constraint : {}), deviceId: { exact: id } });

  // Put `track` in place of `old` in its stream and in the calls sending it.
  // The site keeps `old`, so its mute, hang-up and "ended" carry over.
  const replace = async (stream, old, track) => {
    track.enabled = old.enabled;
    for (const pc of connections) {
      if (pc.connectionState === "closed") {
        connections.delete(pc);
        continue;
      }
      for (const sender of pc.getSenders()) if (sender.track === old) await sender.replaceTrack(track);
    }
    stream.removeTrack(old);
    stream.addTrack(track);
    // Stopped first: from then on, stopping `old` stops `track`.
    old.stop();
    replaced.set(old, track);
    track.addEventListener("ended", () => old.dispatchEvent(new Event("ended")));
  };

  // Put a track from the wanted device in place of each live one of `kind`,
  // in its stream and in the calls sending it.
  const swap = async (kind) => {
    const id = await idFor(kind);
    if (!id || !getUserMedia) return;
    const media = kind === "microphone" ? "audio" : "video";
    for (const stream of streams) {
      if (!stream.getTracks().some((t) => t.readyState === "live")) {
        streams.delete(stream);
        continue;
      }
      for (const old of media === "audio" ? stream.getAudioTracks() : stream.getVideoTracks()) {
        if (old.readyState !== "live" || old.getSettings().deviceId === id) continue;
        const fresh = await getUserMedia.call(navigator.mediaDevices, { [media]: pin(old.getConstraints(), id) });
        await replace(stream, old, fresh.getTracks()[0]);
      }
    }
  };

  // Ask the picker again, and send what's picked in place of the newest share.
  const switchShare = async () => {
    for (const share of shares) {
      if (!share.stream.getTracks().some((t) => t.readyState === "live")) shares.delete(share);
    }
    const share = [...shares].at(-1);
    if (!share || !displayMedia) return;
    const fresh = await displayMedia.call(navigator.mediaDevices, share.constraints);
    for (const track of fresh.getTracks()) {
      const old = share.stream.getTracks().find((t) => t.kind === track.kind && t.readyState === "live");
      // A call sends only the kinds it was set up with.
      if (old) await replace(share.stream, old, track);
      else track.stop();
    }
  };
  // Pickers only open right after a real key, so the browser sends one.
  addEventListener(
    "keydown",
    (e) => {
      if (e.key !== "F23" || Date.now() > shareArmed) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      shareArmed = 0;
      switchShare().catch(() => {});
    },
    true,
  );

  const speakTo = async (target) => {
    const id = await idFor("speaker");
    const sink = target instanceof HTMLMediaElement ? elementSink : contextSink;
    if (id && sink && target.sinkId !== id) await sink.call(target, id).catch(() => {});
  };
  const speakAll = () => {
    for (const element of document.querySelectorAll("audio, video")) outputs.add(new WeakRef(element));
    for (const ref of outputs) {
      const target = ref.deref();
      if (target) speakTo(target);
      else outputs.delete(ref);
    }
  };
  const output = (target) => {
    outputs.add(new WeakRef(target));
    if (wanted.speaker) speakTo(target);
  };
  // Calls often play through autoplaying elements, which never call play().
  const observe = () => {
    if (observer) return;
    observer = new MutationObserver((records) => {
      for (const record of records) {
        for (const node of record.addedNodes) if (node instanceof HTMLMediaElement) output(node);
      }
    });
    observer.observe(document, { childList: true, subtree: true });
  };

  const tracked = (Native, add) =>
    new Proxy(Native, {
      construct(target, args, newTarget) {
        const made = Reflect.construct(target, args, newTarget);
        add(made);
        return made;
      },
    });

  let wrapped = false;
  const install = () => {
    // Chromium exposes navigator.mediaDevices before the MediaDevices name.
    const proto = globalThis.MediaDevices?.prototype ?? (navigator.mediaDevices && Object.getPrototypeOf(navigator.mediaDevices));
    if (wrapped || !proto?.getUserMedia) return;
    wrapped = true;
    getUserMedia = proto.getUserMedia;
    proto.getUserMedia = {
      async getUserMedia(constraints) {
        const asked = { ...constraints };
        const [camera, microphone] = await Promise.all([
          asked.video ? idFor("camera") : null,
          asked.audio ? idFor("microphone") : null,
        ]);
        if (camera) asked.video = pin(asked.video, camera);
        if (microphone) asked.audio = pin(asked.audio, microphone);
        const stream = await getUserMedia.call(this, asked);
        streams.add(stream);
        // Names stay hidden until the site may use a device, so pin it now.
        if (asked.video && wanted.camera && !camera) swap("camera").catch(() => {});
        if (asked.audio && wanted.microphone && !microphone) swap("microphone").catch(() => {});
        if (wanted.speaker) speakAll();
        return stream;
      },
    }.getUserMedia;
    displayMedia = proto.getDisplayMedia;
    if (displayMedia) {
      proto.getDisplayMedia = {
        async getDisplayMedia(constraints) {
          const stream = await displayMedia.call(this, constraints);
          shares.add({ stream, constraints });
          return stream;
        },
      }.getDisplayMedia;
    }

    // The site keeps the track it was given: its mute and hang-up reach the one in its place.
    const enabled = Object.getOwnPropertyDescriptor(MediaStreamTrack.prototype, "enabled");
    Object.defineProperty(MediaStreamTrack.prototype, "enabled", {
      ...enabled,
      set(value) {
        enabled.set.call(this, value);
        const now = replaced.get(this);
        if (now) now.enabled = value;
      },
    });
    const stop = MediaStreamTrack.prototype.stop;
    MediaStreamTrack.prototype.stop = {
      stop() {
        stop.call(this);
        replaced.get(this)?.stop();
      },
    }.stop;

    if (globalThis.RTCPeerConnection) {
      globalThis.RTCPeerConnection = tracked(globalThis.RTCPeerConnection, (pc) => connections.add(pc));
    }

    const play = HTMLMediaElement.prototype.play;
    HTMLMediaElement.prototype.play = {
      play(...args) {
        output(this);
        return play.apply(this, args);
      },
    }.play;
    elementSink = HTMLMediaElement.prototype.setSinkId;
    if (elementSink) {
      HTMLMediaElement.prototype.setSinkId = {
        setSinkId(id) {
          if (!wanted.speaker) return elementSink.call(this, id);
          return idFor("speaker").then((ours) => elementSink.call(this, ours ?? id));
        },
      }.setSinkId;
    }
    if (globalThis.AudioContext) {
      contextSink = AudioContext.prototype.setSinkId;
      globalThis.AudioContext = tracked(globalThis.AudioContext, output);
    }
  };
  install();
  queueMicrotask(install);
  addEventListener("DOMContentLoaded", install, { once: true, capture: true });

  return {
    set(kind, name) {
      if (!(kind in wanted) || wanted[kind] === name) return;
      wanted[kind] = name;
      if (!name) return;
      if (kind === "speaker") {
        observe();
        speakAll();
      } else {
        swap(kind).catch(() => {});
      }
    },
    share() {
      shareArmed = Date.now() + 5000;
    },
    list() {
      const devices = navigator.mediaDevices ? navigator.mediaDevices.enumerateDevices() : Promise.resolve([]);
      devices.then(
        (all) => report(JSON.stringify(all.map((d) => ({ kind: d.kind, label: d.label })))),
        () => report("[]"),
      );
    },
  };
})
