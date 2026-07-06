import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";

/**
 * @typedef {Object} Target
 * @property {PlatformKey} key
 * @property {string} name
 */

/** @type {Target[]} */
const TARGETS = [
  { key: "darwin-x64", name: "@shiinasaku/hayate-darwin-x64" },
  { key: "darwin-arm64", name: "@shiinasaku/hayate-darwin-arm64" },
  { key: "linux-x64", name: "@shiinasaku/hayate-linux-x64" },
  { key: "linux-arm64", name: "@shiinasaku/hayate-linux-arm64" },
  { key: "win32-x64", name: "@shiinasaku/hayate-win32-x64" },
  { key: "win32-arm64", name: "@shiinasaku/hayate-win32-arm64" },
  { key: "android-x64", name: "@shiinasaku/hayate-android-x64" },
  { key: "android-arm64", name: "@shiinasaku/hayate-android-arm64" },
];

const BINARY_NAMES = {
  "darwin-x64": "hayate",
  "darwin-arm64": "hayate",
  "linux-x64": "hayate",
  "linux-arm64": "hayate",
  "win32-x64": "hayate.exe",
  "win32-arm64": "hayate.exe",
  "android-x64": "hayate",
  "android-arm64": "hayate",
};

/**
 * Return the npm package name for the current host platform/architecture.
 *
 * @returns {string}
 */
export function pkgName() {
  const key = `${process.platform}-${process.arch}`;
  const target = TARGETS.find((t) => t.key === key);
  if (!target) {
    const supported = TARGETS.map((t) => t.key).join(", ");
    throw new Error(
      `Hayate has no prebuilt binary for ${process.platform} ${process.arch}. ` +
        `Supported platforms: ${supported}. Build from source: https://github.com/ShiinaSaku/Hayate`,
    );
  }
  return target.name;
}

/**
 * Return the absolute path to the native `hayate` binary installed by the
 * optional platform dependency.
 *
 * @returns {Promise<string>}
 */
export async function binaryPath() {
  const name = pkgName();
  const require = createRequire(import.meta.url);
  const pkgJsonPath = require.resolve(`${name}/package.json`);
  const pkgJson = JSON.parse(await readFile(pkgJsonPath, "utf8"));
  const key = `${process.platform}-${process.arch}`;
  const binary = pkgJson.binary ?? BINARY_NAMES[key];
  if (!binary) {
    throw new Error(`Unknown binary name for ${process.platform} ${process.arch}`);
  }
  return resolve(dirname(pkgJsonPath), binary);
}
