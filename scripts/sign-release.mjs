// Signs release files for the in-app updater (src-tauri/src/update.rs), writing
// `<file>.sig` next to each one. Upload those together with the file.
//
//   pnpm sign-release path/to/Wowl.exe [more files…]
//
// Key: $TAURI_SIGNING_PRIVATE_KEY_PATH, else ~/.tauri/wowl-updater.key.
// Password: $TAURI_SIGNING_PRIVATE_KEY_PASSWORD (empty if the key has none).
// Sign both release files the updater may download: the portable Wowl.exe and
// the NSIS setup (Wowl_<version>_x64-setup.exe).

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const files = process.argv.slice(2);
if (files.length === 0) {
  console.error("usage: pnpm sign-release <file> [more files…]");
  process.exit(2);
}

const key = process.env.TAURI_SIGNING_PRIVATE_KEY_PATH || join(homedir(), ".tauri", "wowl-updater.key");
if (!existsSync(key)) {
  console.error(`signing key not found: ${key}\nset TAURI_SIGNING_PRIVATE_KEY_PATH or create one with 'pnpm tauri signer generate'`);
  process.exit(1);
}
const password = process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "";

for (const file of files) {
  if (!existsSync(file)) {
    console.error(`no such file: ${file}`);
    process.exit(1);
  }
  // shell:true because pnpm is a .cmd shim on Windows
  const r = spawnSync(`pnpm tauri signer sign -f "${key}" -p "${password}" "${file}"`, {
    shell: true,
    stdio: ["ignore", "inherit", "inherit"],
  });
  if (r.status !== 0) process.exit(r.status ?? 1);
  console.log(`signed ${file} -> ${file}.sig`);
}
