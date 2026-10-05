// :navigate prev/next. Returns the URL of the page's previous or next link as
// JSON (or null): rel links first, then links whose text matches one of
// `regexes` (hints.prev_regexes or hints.next_regexes), in order.
(function (which, regexes) {
  const rel = document.querySelector(`link[rel~=${which}][href], a[rel~=${which}][href]`);
  if (rel) return JSON.stringify(rel.href);
  const patterns = regexes.flatMap((r) => {
    try { return [new RegExp(r, "i")]; } catch (_) { return []; }
  });
  const links = Array.from(document.querySelectorAll("a[href]"))
    .filter((a) => a.getClientRects().length && !a.href.startsWith("javascript:"));
  for (const pattern of patterns) {
    const link = links.find((a) => pattern.test(a.textContent.trim()) || pattern.test(a.getAttribute("aria-label") || ""));
    if (link) return JSON.stringify(link.href);
  }
  return JSON.stringify(null);
})
