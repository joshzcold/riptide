// Hint overlay. Installs window.__hbHints once per document; every call
// returns a JSON string because that is what the eval channel carries.
(function () {
  if (window.__hbHints) return;

  const SELECTORS = {
    all: [
      "a", "area", "textarea", "select", "input:not([type=hidden])", "button", "iframe", "summary",
      "[contenteditable]:not([contenteditable=false])", "[onclick]", "[onmousedown]",
      "[role=link]", "[role=option]", "[role=button]", "[role=tab]", "[role=checkbox]",
      "[role=switch]", "[role=menuitem]", "[role=menuitemcheckbox]", "[role=menuitemradio]",
      "[role=treeitem]", "[aria-haspopup]", "[tabindex]:not([tabindex='-1'])",
    ],
    links: ["a[href]", "area[href]", "[role=link][href]"],
    images: ["img"],
    inputs: [
      "input:not([type])", "input[type=text]", "input[type=search]", "input[type=email]",
      "input[type=url]", "input[type=tel]", "input[type=password]", "input[type=number]",
      "input[type=date]", "input[type=datetime-local]", "input[type=month]", "input[type=time]",
      "input[type=week]", "textarea", "[contenteditable]:not([contenteditable=false])",
    ],
  };

  const STYLE = `
    :host { all: initial; }
    .label {
      position: fixed;
      z-index: 2147483647;
      padding: 0 3px;
      border: 1px solid #e3be23;
      border-radius: 3px;
      background: linear-gradient(to bottom, #fff785, #ffc542);
      color: #000;
      font: bold 10pt "DejaVu Sans Mono", monospace;
      line-height: 1.2;
      pointer-events: none;
      white-space: pre;
    }
    .matched { color: #008000; }
  `;

  let elements = [];
  let host = null;
  let labels = [];

  function visibleRect(el) {
    for (const r of el.getClientRects()) {
      if (r.width < 1 || r.height < 1) continue;
      if (r.bottom <= 0 || r.right <= 0 || r.top >= innerHeight || r.left >= innerWidth) continue;
      return r;
    }
    return null;
  }

  function isShown(el) {
    const style = getComputedStyle(el);
    return style.visibility === "visible" && style.display !== "none";
  }

  function urlOf(el) {
    let raw = null;
    if (el instanceof HTMLImageElement) raw = el.currentSrc || el.src;
    else if (el.hasAttribute("href")) raw = el.getAttribute("href");
    if (!raw) return null;
    try {
      const url = new URL(raw, document.baseURI);
      return url.protocol === "javascript:" ? null : url.href;
    } catch {
      return null;
    }
  }

  function clear() {
    if (host) host.remove();
    host = null;
    labels = [];
  }

  window.__hbHints = {
    collect(group) {
      clear();
      const selector = (SELECTORS[group] || SELECTORS.all).join(",");
      elements = Array.from(document.querySelectorAll(selector)).filter((el) => visibleRect(el) && isShown(el));
      return JSON.stringify(elements.map((el) => ({ url: urlOf(el) })));
    },

    show(texts) {
      clear();
      host = document.createElement("hb-hints");
      const root = host.attachShadow({ mode: "closed" });
      const style = document.createElement("style");
      style.textContent = STYLE;
      root.append(style);
      labels = texts.map((text, i) => {
        const rect = visibleRect(elements[i]) || elements[i].getBoundingClientRect();
        const label = document.createElement("span");
        label.className = "label";
        label.style.left = `${Math.max(0, rect.left)}px`;
        label.style.top = `${Math.max(0, rect.top)}px`;
        label.textContent = text;
        root.append(label);
        return { text, label };
      });
      document.documentElement.append(host);
      return "";
    },

    filter(typed) {
      for (const { text, label } of labels) {
        const match = text.startsWith(typed);
        label.style.display = match ? "" : "none";
        if (!match) continue;
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

    // Centre of the element's first visible box, in viewport CSS pixels.
    point(i) {
      const el = elements[i];
      const rect = el && el.isConnected ? visibleRect(el) || el.getBoundingClientRect() : null;
      if (!rect) return JSON.stringify(null);
      const x = Math.min(Math.max(rect.left + rect.width / 2, 0), innerWidth - 1);
      const y = Math.min(Math.max(rect.top + rect.height / 2, 0), innerHeight - 1);
      return JSON.stringify({ x, y });
    },
  };
})();
