// Writes the license of every package esbuild put in the bundle, from its metafile:
//   node notices.mjs <metafile> <out>
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [metafile, out] = process.argv.slice(2);
const meta = JSON.parse(readFileSync(metafile, "utf8"));
const names = new Set();
for (const output of Object.values(meta.outputs)) {
  for (const input of Object.keys(output.inputs)) {
    const match = input.match(/node_modules\/((@[^/]+\/)?[^/]+)/);
    if (match) names.add(match[1]);
  }
}
let text = "Third-party code in editor.bundle.js (riptide://edit/), built by scripts/update-editor-bundle.sh.\n";
for (const name of [...names].sort()) {
  const dir = join("node_modules", name);
  const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
  const file = readdirSync(dir).find((f) => /^(licen[cs]e|copying)/i.test(f));
  if (!file) throw new Error(`${name} has no license file`);
  text += `\n${"=".repeat(72)}\n${name} ${pkg.version} (${pkg.license})\n\n${readFileSync(join(dir, file), "utf8").trim()}\n`;
}
writeFileSync(out, text);
