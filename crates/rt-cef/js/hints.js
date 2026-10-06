// Hint overlay. Installs window.__rtHints once per document; every call
// returns a JSON string because that is what the eval channel carries.
(function () {
  if (window.__rtHints) return;

  const STYLE = `
    :host { all: initial; }
    .label {
      position: fixed;
      z-index: 2147483647;
      padding: var(--rt-hints-padding, 0 3px);
      border: 1px solid var(--rt-hints-border, #e3be23);
      border-radius: var(--rt-hints-radius, 3px);
      background: var(--rt-hints-bg, #ffc542);
      color: var(--rt-hints-fg, #000);
      font: var(--rt-font-hints, bold 10pt "DejaVu Sans Mono", monospace);
      line-height: 1.2;
      pointer-events: none;
      white-space: pre;
    }
    .matched { color: var(--rt-hints-match-fg, #006400); }
  `;

  // Each hintable element with the iframes it sits in, outermost first:
  // same-origin iframes are searched too, and their position offsets the
  // element's box into the top page's coordinates.
  let elements = [];
  let host = null;
  let labels = [];

  function frameOffset(frames) {
    let x = 0, y = 0;
    for (const frame of frames) {
      const r = frame.getBoundingClientRect();
      x += r.left + frame.clientLeft;
      y += r.top + frame.clientTop;
    }
    return { x, y };
  }

  // The element's first box that is visible in its own frame and on screen,
  // in top-page viewport coordinates.
  function visibleRect({ el, frames }) {
    const win = el.ownerDocument.defaultView;
    const { x, y } = frameOffset(frames);
    for (const r of el.getClientRects()) {
      if (r.width < 1 || r.height < 1) continue;
      if (r.bottom <= 0 || r.right <= 0 || r.top >= win.innerHeight || r.left >= win.innerWidth) continue;
      const box = { left: r.left + x, top: r.top + y, width: r.width, height: r.height };
      if (box.top + box.height <= 0 || box.left + box.width <= 0 || box.top >= innerHeight || box.left >= innerWidth) continue;
      return box;
    }
    return null;
  }

  function isShown(el) {
    const style = el.ownerDocument.defaultView.getComputedStyle(el);
    return style.visibility === "visible" && style.display !== "none";
  }

  function urlOf(el) {
    let raw = null;
    if (el.tagName === "IMG") raw = el.currentSrc || el.src;
    else if (el.hasAttribute("href")) raw = el.getAttribute("href");
    if (!raw) return null;
    try {
      const url = new URL(raw, el.ownerDocument.baseURI);
      return url.protocol === "javascript:" ? null : url.href;
    } catch {
      return null;
    }
  }

  // What number hints filter on: the visible text, or a label for elements without one.
  function textOf(el) {
    const text = el.innerText || el.value || el.getAttribute("aria-label") || el.alt || el.title || "";
    return String(text).trim().slice(0, 200).toLowerCase();
  }

  // The document inside a same-origin frame; null for cross-origin ones.
  function innerDocument(frame) {
    try {
      return frame.contentDocument;
    } catch {
      return null;
    }
  }

  // Cross-origin frames found while gathering: the browser hints inside them
  // separately and needs to know where they are.
  let crossFrames = [];

  // Elements in `root` (a document or an open shadow root) and in the
  // shadow roots inside it, which querySelectorAll doesn't enter.
  function gatherRoot(root, frames, selector, out) {
    for (const el of root.querySelectorAll(selector)) {
      // Frames are searched (same-origin) or hinted by the browser (cross-origin)
      // instead of hinted themselves.
      if (el.tagName === "IFRAME" || el.tagName === "FRAME") continue;
      const entry = { el, frames };
      if (visibleRect(entry) && isShown(el)) out.push(entry);
    }
    for (const el of root.querySelectorAll("*")) {
      if (el.shadowRoot) gatherRoot(el.shadowRoot, frames, selector, out);
    }
  }

  function gather(doc, frames, selector, out) {
    gatherRoot(doc, frames, selector, out);
    for (const frame of doc.querySelectorAll("iframe, frame")) {
      const inner = innerDocument(frame);
      const shown = visibleRect({ el: frame, frames }) && isShown(frame);
      if (!inner) {
        // Where the frame's content starts, in this frame's viewport.
        const r = frame.getBoundingClientRect();
        const { x, y } = frameOffset(frames);
        crossFrames.push({
          url: frame.src ? new URL(frame.src, frame.ownerDocument.baseURI).href : "",
          name: frame.name || "",
          rect: shown ? { x: x + r.left + frame.clientLeft, y: y + r.top + frame.clientTop } : null,
        });
      } else if (shown) {
        gather(inner, [...frames, frame], selector, out);
      }
    }
    return out;
  }

  function clear() {
    if (host) host.remove();
    host = null;
    labels = [];
  }

  window.__rtHints = {
    // `selector` is a CSS selector list from hints.selectors.
    // Also says whether this frame collects for itself (the top page, or a
    // frame its parent can't see into) and lists the cross-origin frames in it.
    collect(selector) {
      clear();
      let root;
      try {
        root = window.frameElement === null;
      } catch {
        root = true;
      }
      crossFrames = [];
      elements = root ? gather(document, [], selector, []) : [];
      return JSON.stringify({
        root,
        items: elements.map(({ el }) => ({ url: urlOf(el), text: textOf(el) })),
        frames: crossFrames,
      });
    },

    show(texts, uppercase, theme = {}, css = "") {
      clear();
      host = document.createElement("rt-hints");
      // The theme's hint colors, inherited into the shadow root.
      for (const [token, color] of Object.entries(theme)) host.style.setProperty(`--rt-${token}`, color);
      const root = host.attachShadow({ mode: "closed" });
      const style = document.createElement("style");
      // hints.css comes after, so it can restyle .label and .matched.
      style.textContent = STYLE + css;
      root.append(style);
      labels = texts.map((text, i) => {
        // Number hints give elements the text filter hid no label.
        if (!text) return { text, label: null };
        const rect = visibleRect(elements[i]) || elements[i].el.getBoundingClientRect();
        const label = document.createElement("span");
        label.className = "label";
        label.style.left = `${Math.max(0, rect.left)}px`;
        label.style.top = `${Math.max(0, rect.top)}px`;
        label.textContent = text;
        if (uppercase) label.style.textTransform = "uppercase";
        root.append(label);
        return { text, label };
      });
      document.documentElement.append(host);
      return "";
    },

    // `hide` is false for rapid hints with hints.hide_unmatched_rapid_hints off.
    filter(typed, hide = true) {
      for (const { text, label } of labels) {
        if (!label) continue;
        const match = text.startsWith(typed);
        label.style.display = match || !hide ? "" : "none";
        if (!match) {
          label.replaceChildren(text);
          continue;
        }
        const done = document.createElement("span");
        done.className = "matched";
        done.textContent = typed;
        label.replaceChildren(done, text.slice(typed.length));
      }
      return "";
    },

    clear() {
      clear();
      return "";
    },

    // Click an element without real input, when the browser can't tell where
    // this frame is on screen.
    activate(i) {
      const entry = elements[i];
      if (!entry || !entry.el.isConnected) return "gone";
      entry.el.focus?.();
      entry.el.click?.();
      return "";
    },

    // Centre of the element's first visible box, in viewport CSS pixels.
    point(i) {
      const entry = elements[i];
      const rect = entry && entry.el.isConnected ? visibleRect(entry) || entry.el.getBoundingClientRect() : null;
      if (!rect) return JSON.stringify(null);
      const x = Math.min(Math.max(rect.left + rect.width / 2, 0), innerWidth - 1);
      const y = Math.min(Math.max(rect.top + rect.height / 2, 0), innerHeight - 1);
      return JSON.stringify({ x, y });
    },
  };
})();
