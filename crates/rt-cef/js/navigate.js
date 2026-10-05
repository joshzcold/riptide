// :navigate prev/next. Returns the URL of the page's previous or next link as
// JSON (or null): rel links first, then links whose text looks right,
// using qutebrowser's default hints.prev_regexes and hints.next_regexes.
(function (which) {
  const rel = document.querySelector(`link[rel~=${which}][href], a[rel~=${which}][href]`);
  if (rel) return JSON.stringify(rel.href);
  const patterns = which === "next"
    ? [/\bnext\b/i, /\bmore\b/i, /\bnewer\b/i, /\b[>→≫]\b/, /\b(>>|»)\b/, /\bcontinue\b/i]
    : [/\bprev(ious)?\b/i, /\bback\b/i, /\bolder\b/i, /\b[<←≪]\b/, /\b(<<|«)\b/];
  const links = Array.from(document.querySelectorAll("a[href]"))
    .filter((a) => a.getClientRects().length && !a.href.startsWith("javascript:"));
  for (const pattern of patterns) {
    const link = links.find((a) => pattern.test(a.textContent.trim()) || pattern.test(a.getAttribute("aria-label") || ""));
    if (link) return JSON.stringify(link.href);
  }
  return JSON.stringify(null);
})
