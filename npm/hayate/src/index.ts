import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import os from "node:os";
import { dirname, resolve } from "node:path";

interface Target {
  key: PlatformKey;
  name: string;
}

type PlatformKey =
  | "darwin-x64"
  | "darwin-arm64"
  | "linux-x64"
  | "linux-arm64"
  | "win32-x64"
  | "win32-arm64"
  | "android-x64"
  | "android-arm64";

const TARGETS: Target[] = [
  { key: "darwin-x64", name: "@shiinasaku/hayate-darwin-x64" },
  { key: "darwin-arm64", name: "@shiinasaku/hayate-darwin-arm64" },
  { key: "linux-x64", name: "@shiinasaku/hayate-linux-x64" },
  { key: "linux-arm64", name: "@shiinasaku/hayate-linux-arm64" },
  { key: "win32-x64", name: "@shiinasaku/hayate-win32-x64" },
  { key: "win32-arm64", name: "@shiinasaku/hayate-win32-arm64" },
  { key: "android-x64", name: "@shiinasaku/hayate-android-x64" },
  { key: "android-arm64", name: "@shiinasaku/hayate-android-arm64" },
];

const BINARY_NAMES: Record<PlatformKey, string> = {
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
 * `process.platform` reports `linux` on Android, so extra signals are needed
 * to distinguish it from a normal Linux host.
 */
export function isAndroid(): boolean {
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

/** Platform key for the current host, accounting for Android. */
function hostKey(): string {
  if (isAndroid()) return `android-${process.arch}`;
  return `${process.platform}-${process.arch}`;
}

/** npm package name carrying the prebuilt binary for this host. */
export function pkgName(): string {
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
 * Absolute path to the native `hayate` binary installed by the optional
 * platform dependency.
 */
export async function binaryPath(): Promise<string> {
  const name = pkgName();
  const require = createRequire(import.meta.url);
  const pkgJsonPath = require.resolve(`${name}/package.json`);
  const pkgJson = JSON.parse(await readFile(pkgJsonPath, "utf8")) as { binary?: string };
  const key = hostKey() as PlatformKey;
  const binary = pkgJson.binary ?? BINARY_NAMES[key];
  if (!binary) {
    throw new Error(`Unknown binary name for ${process.platform} ${process.arch}`);
  }
  return resolve(dirname(pkgJsonPath), binary);
}
