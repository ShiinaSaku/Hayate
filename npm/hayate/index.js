import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import os from "node:os";

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
 * Detect whether the current process is running under Android (e.g. Termux).
 *
 * `process.platform` reports `linux` on Android, so extra signals are needed to
 * distinguish it from a normal Linux host.
 *
 * @returns {boolean}
 */
export function isAndroid() {
  if (process.platform !== "linux") return false;
  if (process.env.ANDROID_ROOT || process.env.ANDROID_DATA) return true;
  try {
    return (
      existsSync("/system/build.prop") ||
      (typeof os.release === "function" && os.release().toLowerCase().includes("android"))
    );
  } catch {
    return false;
  }
}

/**
 * Return the platform key for the current host, accounting for Android.
 *
 * @returns {string}
 */
function hostKey() {
  if (isAndroid()) return `android-${process.arch}`;
  return `${process.platform}-${process.arch}`;
}

/**
 * Return the npm package name for the current host platform/architecture.
 *
 * @returns {string}
 */
export function pkgName() {
  const key = hostKey();
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
  const key = hostKey();
  const binary = pkgJson.binary ?? BINARY_NAMES[key];
  if (!binary) {
    throw new Error(`Unknown binary name for ${process.platform} ${process.arch}`);
  }
  return resolve(dirname(pkgJsonPath), binary);
}
