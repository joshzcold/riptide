// Where the word at the text cursor is, so a right-click there makes Chromium
// report its spelling suggestions. Returns JSON {x, y} in CSS pixels, or null.
(function () {
  const el = document.activeElement;
  if (!el) return JSON.stringify(null);
  const isWord = (c) => /[\p{L}\p{M}\p{N}'’-]/u.test(c);
  const center = (r) => ({ x: r.left + r.width / 2, y: r.top + r.height / 2 });

  if (el.isContentEditable) {
    const sel = getSelection();
    if (!sel.rangeCount) return JSON.stringify(null);
    const range = sel.getRangeAt(0).cloneRange();
    const node = range.startContainer;
    if (node.nodeType !== Node.TEXT_NODE) return JSON.stringify(null);
    const text = node.data;
    let start = range.startOffset, end = start;
    while (start > 0 && isWord(text[start - 1])) start--;
    while (end < text.length && isWord(text[end])) end++;
    if (start === end) return JSON.stringify(null);
    range.setStart(node, start);
    range.setEnd(node, end);
    return JSON.stringify(center(range.getBoundingClientRect()));
  }

  if (!(el instanceof HTMLTextAreaElement || el instanceof HTMLInputElement)) {
    return JSON.stringify(null);
  }
  const text = el.value;
  let start = el.selectionStart ?? 0, end = start;
  while (start > 0 && isWord(text[start - 1])) start--;
  while (end < text.length && isWord(text[end])) end++;
  if (start === end) return JSON.stringify(null);

  // Inputs don't expose text geometry, so lay the text out again in a copy of
  // the field and measure the word there.
  const style = getComputedStyle(el);
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
  const rect = el.getBoundingClientRect();
  Object.assign(mirror.style, {
    position: "fixed", left: rect.left + "px", top: rect.top + "px", visibility: "hidden",
    whiteSpace: el instanceof HTMLInputElement ? "pre" : "pre-wrap", wordWrap: "break-word",
    overflow: "hidden",
  });
  mirror.textContent = text.slice(0, start);
  const word = document.createElement("span");
  word.textContent = text.slice(start, end);
  mirror.appendChild(word);
  document.body.appendChild(mirror);
  mirror.scrollTop = el.scrollTop;
  mirror.scrollLeft = el.scrollLeft;
  const point = center(word.getBoundingClientRect());
  mirror.remove();
  return JSON.stringify(point);
})()
