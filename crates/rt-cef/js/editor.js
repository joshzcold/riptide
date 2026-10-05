// :open-editor. `__rtEditor.take()` remembers the focused text field and
// returns its text and cursor as JSON; `__rtEditor.put(id, text)` writes the
// edited text back and tells the page with an input event.
(function () {
  if (window.__rtEditor) return;
  const fields = new Map();
  let next = 1;
  window.__rtEditor = {
    take() {
      const el = document.activeElement;
      if (!el) return JSON.stringify(null);
      let text, offset;
      if (el instanceof HTMLTextAreaElement || (el instanceof HTMLInputElement && el.type !== "password")) {
        text = el.value;
        offset = el.selectionStart ?? 0;
      } else if (el.isContentEditable) {
        text = el.innerText;
        offset = 0;
      } else {
        return JSON.stringify(null);
      }
      const before = text.slice(0, offset).split("\n");
      const id = next++;
      fields.set(id, el);
      return JSON.stringify({ id, text, line: before.length, column: before[before.length - 1].length + 1 });
    },
    put(id, text) {
      const el = fields.get(id);
      fields.delete(id);
      if (!el || !el.isConnected) return JSON.stringify(false);
      if (el.isContentEditable) el.innerText = text;
      else el.value = text;
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
      return JSON.stringify(true);
    },
  };
})();
