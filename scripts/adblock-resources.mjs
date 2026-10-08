// Builds adblock-rust's resources (scriptlets and $redirect files) from an
// unpacked uBlock Origin release: node scripts/adblock-resources.mjs <dir> <out.json>
// <dir> holds uBlock0.chromium's js/ and web_accessible_resources/.
// uBlock Origin is GPL-3.0, like riptide.
import { readFileSync, writeFileSync } from "node:fs";
import { join, extname } from "node:path";
import { pathToFileURL } from "node:url";

const [dir, out] = process.argv.slice(2);
if (!dir || !out) {
  console.error("usage: adblock-resources.mjs <uBlock0.chromium dir> <out.json>");
  process.exit(2);
}
const base64 = (text) => Buffer.from(text).toString("base64");
const resources = [];

// Scriptlets: uBlock Origin registers each as { name, aliases, fn,
// dependencies, requiresTrust }. Ones needing a trusted list get permission
// bit 1, which riptide gives only uBlock Origin's own lists.
const { builtinScriptlets } = await import(pathToFileURL(join(dir, "js/resources/scriptlets.js")).href);
const names = new Set(builtinScriptlets.map((s) => s.name));
for (const s of builtinScriptlets) {
  const helper = !s.name.endsWith(".js");
  const missing = (s.dependencies || []).filter((d) => !names.has(d));
  if (missing.length) throw new Error(`${s.name}: unknown dependencies ${missing}`);
  resources.push({
    name: s.name,
    aliases: s.aliases || [],
    kind: { mime: helper ? "fn/javascript" : "application/javascript" },
    content: base64(s.fn.toString()),
    dependencies: s.dependencies || [],
    ...(s.requiresTrust ? { permission: 1 } : {}),
  });
}

// $redirect targets: files in web_accessible_resources, named (and aliased)
// by js/redirect-resources.js.
const MIME = {
  ".css": "text/css", ".gif": "image/gif", ".html": "text/html", ".js": "application/javascript",
  ".json": "application/json", ".mp3": "audio/mp3", ".mp4": "video/mp4", ".png": "image/png",
  ".txt": "text/plain", ".xml": "text/xml",
};
const { default: redirects } = await import(pathToFileURL(join(dir, "js/redirect-resources.js")).href);
for (const [name, props] of redirects) {
  const mime = MIME[extname(name)];
  if (!mime) continue;
  let content;
  try {
    content = readFileSync(join(dir, "web_accessible_resources", name));
  } catch {
    continue;
  }
  const alias = props.alias === undefined ? [] : [].concat(props.alias);
  resources.push({ name, aliases: alias, kind: { mime }, content: content.toString("base64") });
}

resources.sort((a, b) => a.name.localeCompare(b.name));
writeFileSync(out, JSON.stringify(resources) + "\n");
console.log(`${resources.length} resources written to ${out}`);
