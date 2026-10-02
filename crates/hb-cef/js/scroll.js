// Called as (op, x, y): op is "by" (pixels), "page" (fractions of the viewport)
// or "perc" (absolute percentage; a null axis is left unchanged).
(function (op, x, y) {
  const hb = (window.__hb = window.__hb || {});
  const root = document.scrollingElement || document.documentElement;

  const overflows = (el) => el.scrollHeight > el.clientHeight + 1 || el.scrollWidth > el.clientWidth + 1;
  const isScroller = (el) => {
    const style = getComputedStyle(el);
    return /auto|scroll|overlay/.test(style.overflowY + style.overflowX) && overflows(el);
  };

  // Pages like web mail keep the document fixed and scroll an inner element,
  // so fall back to the largest visible scroller when the root cannot move.
  function target() {
    if (overflows(root)) return root;
    if (hb.scroller && hb.scroller.isConnected && overflows(hb.scroller)) return hb.scroller;
    let best = null;
    let bestArea = 0;
    for (const el of document.body ? document.body.querySelectorAll("*") : []) {
      const area = el.clientWidth * el.clientHeight;
      if (area > bestArea && isScroller(el)) {
        best = el;
        bestArea = area;
      }
    }
    hb.scroller = best;
    return best || root;
  }

  const el = target();
  const view = el === root ? { w: window.innerWidth, h: window.innerHeight } : { w: el.clientWidth, h: el.clientHeight };
  if (op === "by") {
    el.scrollBy({ left: x, top: y, behavior: "instant" });
  } else if (op === "page") {
    el.scrollBy({ left: x * view.w, top: y * view.h, behavior: "instant" });
  } else if (op === "perc") {
    const opts = { behavior: "instant" };
    if (x != null) opts.left = ((el.scrollWidth - view.w) * x) / 100;
    if (y != null) opts.top = ((el.scrollHeight - view.h) * y) / 100;
    el.scrollTo(opts);
  }
})
