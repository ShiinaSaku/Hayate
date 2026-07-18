/**
 * Single source of truth for release targets shared by:
 *   - build.ts (binary archives / deb)
 *   - scripts/npm-release.mts (scoped npm packages)
 *
 * Keep npm package names and archive formats in sync here only.
 */

export type ArchiveKind = "tar.gz" | "zip";

export interface ReleaseTarget {
  /** Rust target triple */
  triple: string;
  /** Short arch label for deb / display */
  arch: "x86_64" | "aarch64";
  os: "linux" | "macos" | "windows" | "android";
  archive: ArchiveKind;
  /** Prefer cargo-zigbuild when cross-compiling glibc Linux */
  zigbuild?: boolean;
  /** Build .deb when --deb is passed */
  deb?: boolean;
  /** Android NDK target */
  ndk?: boolean;
  /** Scoped npm package name for this platform binary */
  npmPkg: string;
  /** npm `os` field */
  npmOs: "darwin" | "linux" | "win32" | "android";
  /** npm `cpu` field */
  npmCpu: "x64" | "arm64";
  /** Binary filename inside the archive */
  binary: string;
}

export const RELEASE_TARGETS: ReleaseTarget[] = [
  {
    triple: "x86_64-unknown-linux-gnu",
    arch: "x86_64",
    os: "linux",
    archive: "tar.gz",
    zigbuild: true,
    deb: true,
    npmPkg: "@shiinasaku/hayate-linux-x64",
    npmOs: "linux",
    npmCpu: "x64",
    binary: "hayate",
  },
  {
    triple: "aarch64-unknown-linux-gnu",
    arch: "aarch64",
    os: "linux",
    archive: "tar.gz",
    zigbuild: true,
    deb: true,
    npmPkg: "@shiinasaku/hayate-linux-arm64",
    npmOs: "linux",
    npmCpu: "arm64",
    binary: "hayate",
  },
  {
    triple: "x86_64-apple-darwin",
    arch: "x86_64",
    os: "macos",
    archive: "tar.gz",
    npmPkg: "@shiinasaku/hayate-darwin-x64",
    npmOs: "darwin",
    npmCpu: "x64",
    binary: "hayate",
  },
  {
    triple: "aarch64-apple-darwin",
    arch: "aarch64",
    os: "macos",
    archive: "tar.gz",
    npmPkg: "@shiinasaku/hayate-darwin-arm64",
    npmOs: "darwin",
    npmCpu: "arm64",
    binary: "hayate",
  },
  {
    triple: "x86_64-pc-windows-msvc",
    arch: "x86_64",
    os: "windows",
    archive: "zip",
    npmPkg: "@shiinasaku/hayate-win32-x64",
    npmOs: "win32",
    npmCpu: "x64",
    binary: "hayate.exe",
  },
  {
    triple: "aarch64-pc-windows-msvc",
    arch: "aarch64",
    os: "windows",
    archive: "zip",
    npmPkg: "@shiinasaku/hayate-win32-arm64",
    npmOs: "win32",
    npmCpu: "arm64",
    binary: "hayate.exe",
  },
  {
    triple: "aarch64-linux-android",
    arch: "aarch64",
    os: "android",
    archive: "tar.gz",
    ndk: true,
    npmPkg: "@shiinasaku/hayate-android-arm64",
    npmOs: "android",
    npmCpu: "arm64",
    binary: "hayate",
  },
  {
    triple: "x86_64-linux-android",
    arch: "x86_64",
    os: "android",
    archive: "tar.gz",
    ndk: true,
    npmPkg: "@shiinasaku/hayate-android-x64",
    npmOs: "android",
    npmCpu: "x64",
    binary: "hayate",
  },
];

export const AUTHOR = {
  name: "Saku Shiina",
  email: "saku@shiina.xyz",
  url: "https://shiina.xyz",
} as const;

export const SITE_URL = "https://hayate.shiina.xyz";
export const REPO = "ShiinaSaku/Hayate";
