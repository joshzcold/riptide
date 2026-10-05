// Caret mode: a keyboard-driven text cursor and selection on any page.
// Chromium only draws a caret in editable text, so this draws its own.
// Every method returns a JSON string because that is what the eval channel carries.
(function () {
  if (window.__rtCaret) return;
  let selecting = false;
  let caret = null;
  const sel = () => getSelection();

  // The first text near the top of the viewport, as [node, offset].
  function startPoint() {
    for (let y = 8; y < innerHeight; y += 12) {
      for (const x of [8, innerWidth / 4, innerWidth / 2]) {
        const p = document.caretPositionFromPoint(x, y);
        if (p && p.offsetNode.nodeType === Node.TEXT_NODE && p.offsetNode.data.trim()) {
          return [p.offsetNode, p.offset];
        }
      }
    }
    return null;
  }

  function focusRect() {
    const s = sel();
    if (!s.focusNode) return null;
    const r = document.createRange();
    r.setStart(s.focusNode, s.focusOffset);
    r.collapse(true);
    const rects = r.getClientRects();
    if (rects.length) return rects[0];
    const el = s.focusNode.nodeType === Node.ELEMENT_NODE ? s.focusNode : s.focusNode.parentElement;
    return el ? el.getBoundingClientRect() : null;
  }

  function draw() {
    const rect = focusRect();
    if (!rect) return;
    if (rect.top < 0 || rect.bottom > innerHeight) scrollBy(0, rect.top - innerHeight / 3);
    const now = focusRect();
    if (!caret) {
      caret = document.createElement("div");
      caret.setAttribute("aria-hidden", "true");
      Object.assign(caret.style, {
        position: "fixed", width: "2px", background: "#e33", zIndex: "2147483647",
        pointerEvents: "none",
      });
      document.documentElement.appendChild(caret);
    }
    Object.assign(caret.style, {
      left: `${now.left}px`, top: `${now.top}px`, height: `${Math.max(now.height, 12)}px`,
    });
  }

  function modify(direction, granularity) {
    sel().modify(selecting ? "extend" : "move", direction, granularity);
  }

  window.__rtCaret = {
    enter() {
      const s = sel();
      selecting = false;
      const visible = s.rangeCount && (() => {
        const r = focusRect();
        return r && r.bottom > 0 && r.top < innerHeight;
      })();
      if (!visible) {
        const start = startPoint();
        if (!start) return JSON.stringify(false);
        s.collapse(start[0], start[1]);
      } else {
        s.collapse(s.focusNode, s.focusOffset);
      }
      draw();
      return JSON.stringify(true);
    },
    move(direction, granularity, count, kind) {
      for (let i = 0; i < count; i++) {
        if (kind === "next-word") {
          // Chromium's "forward word" stops at the end of the word; go on
          // to the start of the next one.
          modify("forward", "word");
          modify("forward", "word");
          modify("backward", "word");
        } else if (kind === "start-of-next-block") {
          modify("forward", "paragraph");
        } else if (kind === "end-of-next-block") {
          modify("forward", "paragraph");
          modify("forward", "paragraphboundary");
        } else if (kind === "start-of-prev-block") {
          modify("backward", "paragraphboundary");
          modify("backward", "paragraph");
        } else if (kind === "end-of-prev-block") {
          modify("backward", "paragraph");
          modify("forward", "paragraphboundary");
        } else {
          modify(direction, granularity);
        }
      }
      draw();
      return "null";
    },
    toggle(line) {
      const s = sel();
      if (line) {
        selecting = true;
        s.collapse(s.focusNode, s.focusOffset);
        s.modify("move", "backward", "lineboundary");
        s.modify("extend", "forward", "lineboundary");
      } else {
        selecting = !selecting;
        if (!selecting) s.collapse(s.focusNode, s.focusOffset);
      }
      draw();
      return JSON.stringify(selecting);
    },
    drop() {
      const s = sel();
      if (s.focusNode) s.collapse(s.focusNode, s.focusOffset);
      draw();
      return "null";
    },
    reverse() {
      const s = sel();
      if (s.anchorNode) s.setBaseAndExtent(s.focusNode, s.focusOffset, s.anchorNode, s.anchorOffset);
      draw();
      return "null";
    },
    text() {
      return JSON.stringify(sel().toString());
    },
    leave() {
      if (caret) caret.remove();
      caret = null;
      selecting = false;
      sel().removeAllRanges();
      return "null";
    },
  };
})();
