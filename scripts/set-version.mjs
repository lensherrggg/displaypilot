#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const next = process.argv[2];
if (!next || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(next)) {
  console.error("Usage: npm run version:set -- <semver>   e.g. 0.2.0");
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const update = (file, transform) => {
  const path = resolve(root, file);
  const before = readFileSync(path, "utf8");
  const after = transform(before);
  if (before !== after) writeFileSync(path, after);
  console.log(`updated ${file}`);
};

update("package.json", (text) => text.replace(/("version"\s*:\s*)"[^"]+"/, `$1"${next}"`));
update("src-tauri/tauri.conf.json", (text) => text.replace(/("version"\s*:\s*)"[^"]+"/, `$1"${next}"`));
update("src-tauri/Cargo.toml", (text) =>
  text.replace(/^(version\s*=\s*)"[^"]+"/m, `$1"${next}"`),
);

try {
  update("src-tauri/Cargo.lock", (text) =>
    text.replace(
      /(\[\[package\]\]\nname = "displaypilot"\nversion = )"[^"]+"/,
      `$1"${next}"`,
    ),
  );
} catch {
  // Cargo.lock is generated on first build.
}

console.log(`\nVersion set to ${next}. Add a "## [${next}]" section to CHANGELOG.md, then run npm run check:version.`);
