#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (file) => readFileSync(resolve(root, file), "utf8");

const packageJson = JSON.parse(read("package.json"));
const tauri = JSON.parse(read("src-tauri/tauri.conf.json"));
const cargo = read("src-tauri/Cargo.toml");
const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const changelog = read("CHANGELOG.md");

const versions = {
  "package.json": packageJson.version,
  "src-tauri/tauri.conf.json": tauri.version,
  "src-tauri/Cargo.toml": cargoVersion,
};

try {
  const lock = read("src-tauri/Cargo.lock");
  const lockVersion = lock.match(
    /\[\[package\]\]\nname = "displaypilot"\nversion = "([^"]+)"/,
  )?.[1];
  if (lockVersion) versions["src-tauri/Cargo.lock"] = lockVersion;
} catch {
  // Cargo.lock is generated on first build.
}

const unique = new Set(Object.values(versions));
if (unique.size !== 1) {
  console.error("Version mismatch detected:");
  for (const [file, version] of Object.entries(versions)) {
    console.error(`  ${file}: ${version ?? "missing"}`);
  }
  process.exit(1);
}

const version = packageJson.version;
if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error(`Invalid SemVer version: ${version}`);
  process.exit(1);
}

if (!changelog.includes(`## [${version}]`)) {
  console.error(`CHANGELOG.md is missing a "## [${version}]" section.`);
  process.exit(1);
}

console.log(`Version ${version} is consistent across manifests and CHANGELOG.md.`);
