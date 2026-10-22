// One step of :spell-next / :spell-prev in the focused text field. Called as
// step(dir, from, restore): with `restore`, put the text cursor back at
// `restore` and stop. Otherwise move the cursor to the start of the next word
// (dir 1) or previous word (dir -1) from index `from` (the cursor when null),
// scrolled into view, and return JSON {x, y, at, origin} in CSS pixels, where
// `at` is the word's index in the field's text; or null when there's none.
(function (dir, from, restore) {
  const el = document.activeElement;
  if (!el) return JSON.stringify(null);
  const words = (text) => [...text.matchAll(/[\p{L}\p{M}\p{N}'’-]+/gu)];
  const center = (r) => ({ x: r.left + r.width / 2, y: r.top + r.height / 2 });
  const pick = (list, at) =>
    dir > 0 ? list.find((m) => m.index > at) : list.findLast((m) => m.index + m[0].length < at);

  if (el.isContentEditable) {
    // The field's text nodes in order, with where each starts in its text.
    const nodes = [];
    let length = 0;
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      nodes.push({ node: n, start: length });
      length += n.data.length;
    }
    const locate = (index) => {
      const hit = nodes.findLast((n) => n.start <= index) ?? nodes[0];
      return hit && { node: hit.node, offset: Math.min(index - hit.start, hit.node.data.length) };
    };
    const place = (index) => {
      const spot = locate(index);
      if (!spot) return null;
      const range = document.createRange();
      range.setStart(spot.node, spot.offset);
      range.collapse(true);
      const sel = getSelection();
      sel.removeAllRanges();
      sel.addRange(range);
      return spot;
    };
    if (restore !== null) {
      place(restore);
      return JSON.stringify(null);
    }
    const sel = getSelection();
    let origin = 0;
    if (sel.rangeCount) {
      const r = sel.getRangeAt(0);
      const hit = nodes.find((n) => n.node === r.startContainer);
      origin = hit ? hit.start + r.startOffset : 0;
    }
    const text = nodes.map((n) => n.node.data).join("");
    const word = pick(words(text), from ?? origin);
    if (!word) return JSON.stringify(null);
    const spot = place(word.index);
    const range = document.createRange();
    range.setStart(spot.node, spot.offset);
    range.setEnd(spot.node, Math.min(spot.offset + word[0].length, spot.node.data.length));
    spot.node.parentElement?.scrollIntoView({ block: "nearest", inline: "nearest" });
    return JSON.stringify({ ...center(range.getBoundingClientRect()), at: word.index, origin });
  }

  if (!(el instanceof HTMLTextAreaElement || el instanceof HTMLInputElement)) {
    return JSON.stringify(null);
  }
  if (restore !== null) {
    el.setSelectionRange(restore, restore);
    return JSON.stringify(null);
  }
  const origin = el.selectionStart ?? 0;
  const word = pick(words(el.value), from ?? origin);
  if (!word) return JSON.stringify(null);
  el.setSelectionRange(word.index, word.index);

  // Inputs don't expose text geometry, so lay the text out again in a copy of
  // the field and measure the word there, as spell.js does.
  const style = getComputedStyle(el);
  const measure = () => {
    const rect = el.getBoundingClientRect();
    const mirror = document.createElement("div");
    for (const prop of [
      "boxSizing", "width", "height", "overflowX", "overflowY", "borderTopWidth", "borderRightWidth",
      "borderBottomWidth", "borderLeftWidth", "paddingTop", "paddingRight", "paddingBottom",
      "paddingLeft", "fontStyle", "fontVariant", "fontWeight", "fontStretch", "fontSize",
      "lineHeight", "fontFamily", "textAlign", "textTransform", "textIndent", "letterSpacing",
      "wordSpacing", "tabSize",
    ]) {
      mirror.style[prop] = style[prop];
    }
    Object.assign(mirror.style, {
      position: "fixed", left: rect.left + "px", top: rect.top + "px", visibility: "hidden",
      whiteSpace: el instanceof HTMLInputElement ? "pre" : "pre-wrap", wordWrap: "break-word",
      overflow: "hidden",
    });
    mirror.textContent = el.value.slice(0, word.index);
    const span = document.createElement("span");
    span.textContent = word[0];
    mirror.appendChild(span);
    document.body.appendChild(mirror);
    mirror.scrollTop = el.scrollTop;
    mirror.scrollLeft = el.scrollLeft;
    const box = span.getBoundingClientRect();
    const offset = { top: box.top - rect.top + el.scrollTop, left: box.left - rect.left + el.scrollLeft };
    mirror.remove();
    return { box, offset, rect };
  };
  // Scroll the field so the word shows, then measure where it ended up.
  let { box, offset, rect } = measure();
  if (box.top < rect.top || box.bottom > rect.bottom) {
    el.scrollTop = Math.max(0, offset.top - el.clientHeight / 2);
  }
  if (box.left < rect.left || box.right > rect.right) {
    el.scrollLeft = Math.max(0, offset.left - el.clientWidth / 2);
  }
  ({ box } = measure());
  el.scrollIntoView({ block: "nearest", inline: "nearest" });
  ({ box } = measure());
  return JSON.stringify({ ...center(box), at: word.index, origin });
})
