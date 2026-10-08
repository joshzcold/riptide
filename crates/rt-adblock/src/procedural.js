// Procedural and action cosmetic filters (`##.ad:has-text(Sponsored)`,
// `##.x:upward(2)`, `##.y:remove()`), as adblock-rust describes them:
// { selector: [{ type, arg }, ...], action?: { type, arg } }. Each operator
// narrows a set of elements in order; the action (hide by default) applies
// to what's left. Runs before the page's scripts and again as the page
// changes. Called with the filters as an array.
(function (filters) {
  const regex = (text) => {
    const m = /^\/(.+)\/([a-z]*)$/s.exec(text);
    if (m) {
      try { return new RegExp(m[1], m[2]); } catch { return null; }
    }
    return null;
  };
  // A plain string or /regex/: a test for text.
  const matcher = (arg) => {
    const re = regex(arg);
    return re ? (s) => re.test(s) : (s) => s.includes(arg);
  };
  // `name: value` or `"name"="value"`, either side plain or /regex/.
  const pair = (arg, sep) => {
    const at = arg.indexOf(sep);
    const unquote = (s) => s.trim().replace(/^"(.*)"$/s, "$1");
    const name = unquote(at < 0 ? arg : arg.slice(0, at));
    const value = at < 0 ? null : unquote(arg.slice(at + 1));
    return [matcher(name), value === null ? () => true : matcher(value), name];
  };
  const scoped = (node, selector) => {
    const s = selector.trim();
    try {
      // `:scope` makes a following `> .x` or `.x` relative to the node.
      return node === document ? [...document.querySelectorAll(s)] : [...node.querySelectorAll(`:scope ${s}`)];
    } catch { return []; }
  };
  const OPS = {
    "css-selector": (nodes, arg) => nodes.flatMap((n) => scoped(n, arg)),
    "has-text": (nodes, arg) => { const m = matcher(arg); return nodes.filter((n) => m(n.textContent || "")); },
    "min-text-length": (nodes, arg) => nodes.filter((n) => (n.textContent || "").length >= Number(arg)),
    "matches-attr": (nodes, arg) => {
      const [name, value] = pair(arg, "=");
      return nodes.filter((n) => [...(n.attributes || [])].some((a) => name(a.name) && value(a.value)));
    },
    "matches-css": (nodes, arg) => css(nodes, arg, null),
    "matches-css-before": (nodes, arg) => css(nodes, arg, "::before"),
    "matches-css-after": (nodes, arg) => css(nodes, arg, "::after"),
    "matches-path": (nodes, arg) => (matcher(arg)(location.pathname + location.search) ? nodes : []),
    upward: (nodes, arg) => nodes.map((n) => {
      if (/^\d+$/.test(arg)) {
        let up = n;
        for (let i = 0; i < Number(arg) && up; i++) up = up.parentElement;
        return up;
      }
      try { return n.parentElement && n.parentElement.closest(arg); } catch { return null; }
    }).filter(Boolean),
    xpath: (nodes, arg) => nodes.flatMap((n) => {
      try {
        const found = document.evaluate(arg, n, null, XPathResult.ORDERED_NODE_SNAPSHOT_TYPE, null);
        return Array.from({ length: found.snapshotLength }, (_, i) => found.snapshotItem(i)).filter((x) => x instanceof Element);
      } catch { return []; }
    }),
  };
  function css(nodes, arg, pseudo) {
    const at = arg.indexOf(":");
    if (at < 0) return [];
    const property = arg.slice(0, at).trim();
    const value = matcher(arg.slice(at + 1).trim());
    return nodes.filter((n) => value(getComputedStyle(n, pseudo).getPropertyValue(property)));
  }
  const ACTIONS = {
    hide: (el) => el.style.setProperty("display", "none", "important"),
    remove: (el) => el.remove(),
    style: (el, arg) => {
      for (const declaration of arg.split(";")) {
        const at = declaration.indexOf(":");
        if (at > 0) el.style.setProperty(declaration.slice(0, at).trim(), declaration.slice(at + 1).replace(/!important/, "").trim(), "important");
      }
    },
    "remove-attr": (el, arg) => { const m = matcher(arg); for (const a of [...el.attributes]) if (m(a.name)) el.removeAttribute(a.name); },
    "remove-class": (el, arg) => { const m = matcher(arg); for (const c of [...el.classList]) if (m(c)) el.classList.remove(c); },
  };
  const select = (filter) => {
    let nodes = [document];
    for (const op of filter.selector) {
      const run = OPS[op.type];
      if (!run) return [];
      nodes = run(nodes, op.arg);
      if (!nodes.length) return nodes;
    }
    return nodes.filter((n) => n instanceof Element);
  };
  const apply = () => {
    for (const filter of filters) {
      const action = filter.action || { type: "hide" };
      const act = ACTIONS[action.type];
      if (!act) continue;
      for (const el of select(filter)) {
        try { act(el, action.arg); } catch {}
      }
    }
  };
  // Again as the page changes, at most every 100 ms; our own changes don't
  // count, since they're made while the observer is off.
  let pending = false;
  const observer = new MutationObserver(() => {
    if (pending) return;
    pending = true;
    setTimeout(run, 100);
  });
  const watch = () => observer.observe(document.documentElement, { childList: true, subtree: true, characterData: true });
  function run() {
    pending = false;
    observer.disconnect();
    try { apply(); } finally { if (document.documentElement) watch(); }
  }
  if (document.documentElement) watch();
  document.addEventListener("DOMContentLoaded", run, { once: true });
  addEventListener("load", run, { once: true });
})
