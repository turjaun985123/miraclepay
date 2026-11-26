// Design-token drift lint (Waves 158/199, design-audit/10 Gate 0).
// Hex literals may live ONLY in src/styles/tokens.css (primitives),
// src/components/Icon.astro (none expected) and public/favicon.svg.
// Everywhere else they are drift. Ratchet: TOTAL_BASELINE may only go
// DOWN as waves migrate pages onto tokens; Wave 199 target is 0.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const TOTAL_BASELINE = 0;
const root = new URL("../src/", import.meta.url).pathname;
const allowed = new Set(["styles/tokens.css"]);
const hexRe = /#[0-9a-fA-F]{3,8}\b/g;

let total = 0;
const offenders = [];
function walk(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p);
    else if (/\.(astro|ts|css)$/.test(name)) {
      const rel = relative(root, p);
      if (allowed.has(rel)) continue;
      // Strip link targets / ids / anchors so href="#features" and
      // text like "#1042" are not mistaken for colour literals.
      const text = readFileSync(p, "utf8")
        .replace(/(href|id|for|aria-labelledby|value|placeholder)="[^"]*"/g, '$1=""')
        .replace(/#\d{3,}\b/g, "#N") // order/reference numbers in copy
        .replace(/url\(#[^)]*\)/g, "url()");
      const hits = text.match(hexRe) ?? [];
      if (hits.length) { offenders.push(`${rel}: ${hits.length}`); total += hits.length; }
    }
  }
}
walk(root);
console.log(`hex literals outside tokens.css: ${total} (baseline ${TOTAL_BASELINE})`);
for (const o of offenders) console.log("  " + o);
if (total > TOTAL_BASELINE) {
  console.error("FAIL: design-token drift increased. Move colours into tokens.css.");
  process.exit(1);
}
