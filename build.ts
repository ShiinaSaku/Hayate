#!/usr/bin/env bun
/**
 * Cross-platform binary build and packaging script for Hayate.
 *
 * Run with Bun:
 *   bun run build.ts                  # build for the host target
 *   bun run build.ts --all            # build every target this host can reach
 *   bun run build.ts --target x86_64-unknown-linux-gnu
 *   bun run build.ts --release-dir dist
 *   bun run build.ts --deb            # also build .deb packages for Linux targets
 *   bun run build.ts --android        # include Android Termux targets
 *
 * The script:
 *   1. Reads the workspace version from the root Cargo.toml.
 *   2. Builds hayate-cli in release mode for the requested target(s).
 *   3. Generates shell completions from the freshly built binary.
 *   4. Packages the binary + extras into .tar.gz (Unix/Android) or .zip (Windows).
 *   5. Builds .deb packages for Debian/Ubuntu on Linux targets (optional).
 *   6. Writes a SHA256SUMS file for every produced archive.
 */

import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { gzip } from "node:zlib";
import { promisify } from "node:util";

const gzipAsync = promisify(gzip);

interface Target {
  triple: string;
  arch: string;
  os: "linux" | "macos" | "windows" | "android";
  vendor: string;
  env?: string;
  zigbuild?: boolean;
  archive: "tar.gz" | "zip";
  deb?: boolean;
  ndk?: boolean;
}

const TARGETS: Target[] = [
  // Linux (glibc) — cross-compiled with cargo-zigbuild on macOS/Linux.
  {
    triple: "x86_64-unknown-linux-gnu",
    arch: "x86_64",
    os: "linux",
    vendor: "unknown",
    env: "gnu",
    zigbuild: true,
    archive: "tar.gz",
    deb: true,
  },
  {
    triple: "aarch64-unknown-linux-gnu",
    arch: "aarch64",
    os: "linux",
    vendor: "unknown",
    env: "gnu",
    zigbuild: true,
    archive: "tar.gz",
    deb: true,
  },

  // macOS.
  {
    triple: "x86_64-apple-darwin",
    arch: "x86_64",
    os: "macos",
    vendor: "apple",
    archive: "tar.gz",
  },
  {
    triple: "aarch64-apple-darwin",
    arch: "aarch64",
    os: "macos",
    vendor: "apple",
    archive: "tar.gz",
  },

  // Windows.
  {
    triple: "x86_64-pc-windows-msvc",
    arch: "x86_64",
    os: "windows",
    vendor: "pc",
    env: "msvc",
    archive: "zip",
  },
  {
    triple: "aarch64-pc-windows-msvc",
    arch: "aarch64",
    os: "windows",
    vendor: "pc",
    env: "msvc",
    archive: "zip",
  },

  // Android (Termux / generic NDK). Built with cargo-ndk when available,
  // otherwise falls back to the workspace's .cargo/config.toml linker scripts.
  {
    triple: "aarch64-linux-android",
    arch: "aarch64",
    os: "android",
    vendor: "unknown",
    env: "android",
    archive: "tar.gz",
    ndk: true,
  },
  {
    triple: "x86_64-linux-android",
    arch: "x86_64",
    os: "android",
    vendor: "unknown",
    env: "android",
    archive: "tar.gz",
    ndk: true,
  },
];

interface Args {
  target?: string;
  all: boolean;
  releaseDir: string;
  skipExtras: boolean;
  skipChecksums: boolean;
  verbose: boolean;
  deb: boolean;
  android: boolean;
}

function parseArgs(): Args {
  const raw = process.argv.slice(2);
  const args: Args = {
    all: false,
    releaseDir: "dist",
    skipExtras: false,
    skipChecksums: false,
    verbose: false,
    deb: false,
    android: false,
  };

  let i = 0;
  while (i < raw.length) {
    const arg = raw[i];
    const next = () => {
      i += 1;
      const value = raw[i];
      if (value === undefined) {
        console.error(`Missing value for ${arg}`);
        printHelp();
        process.exit(1);
      }
      return value;
    };

    if (arg === "--all") {
      args.all = true;
    } else if (arg === "--target" || arg === "-t") {
      args.target = next();
    } else if (arg === "--release-dir" || arg === "-o") {
      args.releaseDir = next();
    } else if (arg === "--skip-extras") {
      args.skipExtras = true;
    } else if (arg === "--skip-checksums") {
      args.skipChecksums = true;
    } else if (arg === "--verbose" || arg === "-v") {
      args.verbose = true;
    } else if (arg === "--deb") {
      args.deb = true;
    } else if (arg === "--android") {
      args.android = true;
    } else if (arg === "--help" || arg === "-h") {
      printHelp();
      process.exit(0);
    } else {
      console.error(`Unknown argument: ${arg}`);
      printHelp();
      process.exit(1);
    }

    i += 1;
  }

  return args;
}

function printHelp() {
  console.log(`Hayate binary build script

Usage: bun run build.ts [options]

Options:
  --all                Build every target this host can reach
  --target, -t <triple> Build a specific target triple
  --release-dir, -o <dir> Output directory for archives (default: dist)
  --skip-extras        Do not generate shell completions
  --skip-checksums     Do not write SHA256SUMS
  --deb                Build .deb packages for Linux targets
  --android            Include Android (Termux) targets in --all
  --verbose, -v        Print cargo/zigbuild output instead of swallowing it
  --help, -h           Show this help

Supported targets:
${TARGETS.map((t) => `  ${t.triple}`).join("\n")}
`);
}

function log(...args: unknown[]) {
  console.log("[build]", ...args);
}

function fail(...args: unknown[]): never {
  console.error("[build] error:", ...args);
  process.exit(1);
}

async function readVersion(): Promise<string> {
  const cargoToml = await Bun.file(join(import.meta.dir, "Cargo.toml")).text();
  const match = cargoToml.match(/^version\s*=\s*"([^"]+)"/m);
  if (!match?.[1]) fail("could not parse workspace version from Cargo.toml");
  return match[1];
}

async function hostTriple(): Promise<string> {
  const proc = Bun.spawn({
    cmd: ["rustc", "-vV"],
    stdout: "pipe",
    stderr: "pipe",
  });
  const output = await new Response(proc.stdout).text();
  const exit = await proc.exited;
  if (exit !== 0) fail("failed to run rustc -vV");
  const match = output.match(/host:\s*(.+)/);
  if (!match?.[1]) fail("could not determine host triple");
  return match[1].trim();
}

async function hasCommand(cmd: string): Promise<boolean> {
  return Bun.which(cmd) !== null;
}

async function isGnuAr(): Promise<boolean> {
  try {
    const proc = Bun.spawn({ cmd: ["ar", "--version"], stdout: "pipe", stderr: "pipe" });
    const out = await new Response(proc.stdout).text();
    await proc.exited;
    return out.includes("GNU ar");
  } catch {
    return false;
  }
}

async function run(
  cmd: string,
  args: string[],
  options?: { cwd?: string; env?: Record<string, string | undefined>; verbose?: boolean },
): Promise<void> {
  const proc = Bun.spawn({
    cmd: [cmd, ...args],
    cwd: options?.cwd,
    env: { ...process.env, ...options?.env },
    stdout: options?.verbose ? "inherit" : "pipe",
    stderr: options?.verbose ? "inherit" : "pipe",
  });

  const [stdout, stderr] = await Promise.all([
    options?.verbose ? "" : new Response(proc.stdout).text(),
    options?.verbose ? "" : new Response(proc.stderr).text(),
  ]);
  const exit = await proc.exited;

  if (exit !== 0) {
    throw new Error(`${cmd} ${args.join(" ")} exited with ${exit}\n${stdout}\n${stderr}`.trim());
  }
}

function binaryName(target: Target): string {
  return target.os === "windows" ? "hayate.exe" : "hayate";
}

function archiveName(version: string, target: Target): string {
  return `hayate-v${version}-${target.triple}.${target.archive}`;
}

function debArch(target: Target): string {
  return target.arch === "aarch64" ? "arm64" : "amd64";
}

function debName(version: string, target: Target): string {
  return `hayate_${version}_${debArch(target)}.deb`;
}

function canBuild(target: Target, host: string): boolean {
  if (target.triple === host) return true;

  // Windows MSVC requires a Windows host for a proper build.
  if (target.os === "windows" && !host.includes("windows")) return false;

  // macOS targets can be cross-built from another macOS host with the SDK.
  if (target.os === "macos" && host.includes("apple-darwin")) return true;

  // Linux glibc targets from macOS or Linux via cargo-zigbuild.
  if (target.os === "linux") return hasCargoZigbuildSync();

  // Android targets can be built from any host with cargo-ndk or the NDK linker scripts.
  if (target.os === "android") return true;

  return false;
}

let _hasCargoZigbuild: boolean | undefined;
function hasCargoZigbuildSync(): boolean {
  if (_hasCargoZigbuild === undefined) {
    const result = Bun.spawnSync({
      cmd: ["cargo-zigbuild", "--version"],
      stdout: "pipe",
      stderr: "pipe",
    });
    _hasCargoZigbuild = result.success;
  }
  return _hasCargoZigbuild;
}

async function buildNativeHost(host: string, verbose: boolean): Promise<void> {
  const isZig = host.includes("linux") && hasCargoZigbuildSync();
  const builder = isZig ? "cargo-zigbuild" : "cargo";
  await run(builder, ["build", "--release", "--package", "hayate-cli", "--target", host], {
    verbose,
  });
}

async function buildTarget(
  target: Target,
  version: string,
  releaseDir: string,
  verbose: boolean,
  skipExtras: boolean,
  host: string,
  extrasDir?: string,
  buildDeb = false,
): Promise<{ archive?: string; deb?: string }> {
  log(`building ${target.triple}`);

  const builder = await pickBuilder(target);
  const buildArgs = buildArgsFor(target, builder);

  // Ensure the cross-compilation std target is installed so a fresh host
  // doesn't fail mid-build with "can't find crate for std".
  if (target.triple !== host) {
    try {
      await run("rustup", ["target", "add", target.triple], { verbose });
    } catch (err) {
      log(`  could not install target ${target.triple}: ${err}`);
    }
  }

  try {
    await run(builder, buildArgs, { verbose });
  } catch (err) {
    log(`failed to build ${target.triple}: ${err}`);
    return {};
  }

  const binName = binaryName(target);
  const builtPath = join(import.meta.dir, "target", target.triple, "release", binName);
  if (!existsSync(builtPath)) {
    log(`expected binary not found at ${builtPath}`);
    return {};
  }

  const staging = join(releaseDir, ".staging", target.triple);
  rmSync(staging, { recursive: true, force: true });
  mkdirSync(staging, { recursive: true });

  const destBin = join(staging, binName);
  copyFileSync(builtPath, destBin);
  await stripBinary(destBin, target);

  if (!skipExtras && extrasDir) {
    copyExtras(extrasDir, staging);
  }

  const archivePath = join(releaseDir, archiveName(version, target));
  await createArchive(archivePath, staging, target);
  log(`packaged ${archivePath}`);

  let debPath: string | undefined;
  if (buildDeb && target.deb && target.os === "linux") {
    debPath = join(releaseDir, debName(version, target));
    await buildDebPackage(debPath, destBin, version, target, staging, extrasDir);
  }

  rmSync(staging, { recursive: true, force: true });
  return { archive: archivePath, deb: debPath };
}

async function pickBuilder(target: Target): Promise<string> {
  if (target.os === "android") {
    if (await hasCommand("cargo-ndk")) return "cargo-ndk";
    return "cargo";
  }
  if (target.zigbuild && hasCargoZigbuildSync()) return "cargo-zigbuild";
  return "cargo";
}

function buildArgsFor(target: Target, builder: string): string[] {
  if (builder === "cargo-ndk") {
    return [
      "ndk",
      "--target",
      target.triple,
      "--",
      "build",
      "--release",
      "--package",
      "hayate-cli",
    ];
  }
  return ["build", "--release", "--package", "hayate-cli", "--target", target.triple];
}

async function stripBinary(path: string, target: Target): Promise<void> {
  if (target.os === "windows" || target.os === "macos") return;
  const strip = target.triple.startsWith("aarch64-") ? "aarch64-linux-gnu-strip" : "strip";
  if (await hasCommand(strip)) {
    try {
      await run(strip, [path]);
    } catch (err) {
      log(`  could not strip ${path}: ${err}`);
    }
  }
}

async function generateExtras(binPath: string, extrasDir: string, verbose: boolean): Promise<void> {
  if (existsSync(extrasDir)) {
    rmSync(extrasDir, { recursive: true, force: true });
  }
  mkdirSync(extrasDir, { recursive: true });

  const completionsDir = join(extrasDir, "completions");
  mkdirSync(completionsDir, { recursive: true });

  const shells: [string, string][] = [
    ["bash", "hayate.bash"],
    ["zsh", "_hayate"],
    ["fish", "hayate.fish"],
    ["powershell", "_hayate.ps1"],
  ];

  for (const [shell, filename] of shells) {
    try {
      const out = join(completionsDir, filename);
      await captureToFile(binPath, ["completions", shell], out);
    } catch (err) {
      log(`  could not generate ${shell} completions: ${err}`);
    }
  }
}

function copyExtras(extrasDir: string, staging: string): void {
  const completionsSrc = join(extrasDir, "completions");
  const completionsDst = join(staging, "completions");
  if (existsSync(completionsSrc)) {
    mkdirSync(completionsDst, { recursive: true });
    copyDirSync(completionsSrc, completionsDst);
  }
}

function copyDirSync(src: string, dst: string): void {
  for (const entry of readdirSync(src, { withFileTypes: true })) {
    const srcPath = join(src, entry.name);
    const dstPath = join(dst, entry.name);
    if (entry.isDirectory()) {
      mkdirSync(dstPath, { recursive: true });
      copyDirSync(srcPath, dstPath);
    } else {
      copyFileSync(srcPath, dstPath);
    }
  }
}

async function captureToFile(cmd: string, args: string[], outPath: string): Promise<void> {
  const proc = Bun.spawn({ cmd: [cmd, ...args], stdout: "pipe", stderr: "pipe" });
  const output = await new Response(proc.stdout).arrayBuffer();
  const exit = await proc.exited;
  if (exit !== 0) {
    const err = await new Response(proc.stderr).text();
    throw new Error(`${cmd} ${args.join(" ")} exited ${exit}: ${err}`);
  }
  await Bun.write(outPath, output);
}

async function createArchive(archivePath: string, staging: string, target: Target): Promise<void> {
  mkdirSync(dirname(archivePath), { recursive: true });

  if (target.archive === "tar.gz") {
    const tar = await tarEntries(staging);
    const gzipped = await gzipAsync(tar, { level: 9 });
    await Bun.write(archivePath, gzipped);
  } else {
    // Prefer `zip` when available; otherwise use PowerShell on Windows.
    if (await hasCommand("zip")) {
      await run("zip", ["-r", archivePath, "."], { cwd: staging });
    } else if (process.platform === "win32") {
      await run(
        "powershell",
        ["Compress-Archive", "-Path", `${staging}\\*`, "-DestinationPath", archivePath, "-Force"],
        { cwd: staging },
      );
    } else {
      fail("zip command not found and not on Windows");
    }
  }
}

async function tarEntries(staging: string): Promise<Uint8Array> {
  // Use system tar for correct ustar/pax metadata, then compress with Bun.gzip.
  const proc = Bun.spawn({
    cmd: ["tar", "-cf", "-", "-C", staging, "."],
    stdout: "pipe",
    stderr: "pipe",
  });
  const out = await new Response(proc.stdout).arrayBuffer();
  const exit = await proc.exited;
  if (exit !== 0) {
    const err = await new Response(proc.stderr).text();
    throw new Error(`tar failed: ${err}`);
  }
  return new Uint8Array(out);
}

async function buildDebPackage(
  debPath: string,
  binPath: string,
  version: string,
  target: Target,
  staging: string,
  extrasDir?: string,
): Promise<void> {
  log(`building .deb ${basename(debPath)}`);
  const controlDir = join(staging, "..", `.deb-${target.triple}`);
  rmSync(controlDir, { recursive: true, force: true });

  const debianInstall = join(controlDir, "DEBIAN");
  const binInstall = join(controlDir, "usr", "bin");
  const shareDir = join(controlDir, "usr", "share");
  const completionDir = join(shareDir, "bash-completion", "completions");
  const fishDir = join(shareDir, "fish", "vendor_completions.d");
  const zshDir = join(shareDir, "zsh", "vendor-completions");

  mkdirSync(debianInstall, { recursive: true });
  mkdirSync(binInstall, { recursive: true });
  mkdirSync(completionDir, { recursive: true });
  mkdirSync(fishDir, { recursive: true });
  mkdirSync(zshDir, { recursive: true });

  copyFileSync(binPath, join(binInstall, "hayate"));

  if (extrasDir) {
    const bash = join(extrasDir, "completions", "hayate.bash");
    if (existsSync(bash)) {
      await Bun.write(join(completionDir, "hayate"), Bun.file(bash));
    }
    const fish = join(extrasDir, "completions", "hayate.fish");
    if (existsSync(fish)) {
      await Bun.write(join(fishDir, "hayate.fish"), Bun.file(fish));
    }
    const zsh = join(extrasDir, "completions", "_hayate");
    if (existsSync(zsh)) {
      await Bun.write(join(zshDir, "_hayate"), Bun.file(zsh));
    }
  }

  const deps = "libc6 (>= 2.31), zlib1g";

  await Bun.write(
    join(debianInstall, "control"),
    `Package: hayate
Version: ${version}
Section: utils
Priority: optional
Architecture: ${debArch(target)}
Depends: ${deps}
Maintainer: ShiinaSaku <hayate@example.com>
Description: Encrypted, compressed LAN file transfer tool
 Hayate sends files and directories over a local network using QUIC,
 X25519 key exchange, and AEAD encryption.
`,
  );

  if (await hasCommand("dpkg-deb")) {
    await run("dpkg-deb", ["--build", "--root-owner-group", controlDir, debPath]);
  } else if (await isGnuAr()) {
    await buildDebWithAr(controlDir, debPath);
  } else {
    rmSync(controlDir, { recursive: true, force: true });
    throw new Error("dpkg-deb (or GNU ar) required to build .deb packages");
  }

  rmSync(controlDir, { recursive: true, force: true });
}

async function buildDebWithAr(controlDir: string, debPath: string): Promise<void> {
  // Minimal .deb construction without dpkg-deb: debian-binary, control.tar.gz, data.tar.gz.
  const tmp = join(controlDir, "..", "deb-ar");
  mkdirSync(tmp, { recursive: true });

  await Bun.write(join(tmp, "debian-binary"), "2.0\n");

  const controlTar = await tarEntries(join(controlDir, "DEBIAN"));
  await Bun.write(
    join(tmp, "control.tar.gz"),
    Buffer.from(await gzipAsync(controlTar, { level: 9 })),
  );

  const dataDir = join(controlDir, "usr");
  const dataTar = await tarEntries(dataDir);
  await Bun.write(join(tmp, "data.tar.gz"), Buffer.from(await gzipAsync(dataTar, { level: 9 })));

  await run("ar", ["r", debPath, "debian-binary", "control.tar.gz", "data.tar.gz"], {
    cwd: tmp,
  });
  rmSync(tmp, { recursive: true, force: true });
}

async function sha256(filePath: string): Promise<string> {
  const hash = createHash("sha256");
  const stream = Bun.file(filePath).stream();
  const reader = stream.getReader();
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    if (value) hash.update(value);
  }
  return hash.digest("hex");
}

async function writeChecksums(releaseDir: string, artifacts: string[]): Promise<void> {
  const sums = await Promise.all(
    artifacts.map(async (path) => {
      const sum = await sha256(path);
      return `${sum}  ${basename(path)}`;
    }),
  );
  const outPath = join(releaseDir, "SHA256SUMS.txt");
  await Bun.write(outPath, sums.join("\n") + "\n");
  log(`wrote ${outPath}`);
}

async function main() {
  const args = parseArgs();
  const version = await readVersion();
  const host = await hostTriple();
  log(`workspace version: ${version}`);
  log(`host triple: ${host}`);

  mkdirSync(args.releaseDir, { recursive: true });

  // Generate shell completions once using the native host binary.
  let extrasDir: string | undefined;
  if (!args.skipExtras) {
    extrasDir = join(args.releaseDir, ".extras");
    log("building native host binary to generate extras");
    await buildNativeHost(host, args.verbose);
    const hostBin = join(
      import.meta.dir,
      "target",
      host,
      "release",
      host.includes("windows") ? "hayate.exe" : "hayate",
    );
    if (!existsSync(hostBin)) {
      fail(`host binary not found at ${hostBin} after build`);
    }
    await generateExtras(hostBin, extrasDir, args.verbose);
  }

  let targets: Target[];
  if (args.all) {
    targets = TARGETS.filter((t) => {
      if (!canBuild(t, host)) return false;
      if (t.os === "android" && !args.android) return false;
      return true;
    });
    if (targets.length === 0) fail("no supported targets can be built on this host");
    log(`will build ${targets.length} target(s) on this host`);
  } else if (args.target) {
    const found = TARGETS.find((t) => t.triple === args.target);
    if (!found) fail(`unknown target: ${args.target}`);
    targets = [found];
  } else {
    const hostTarget = TARGETS.find((t) => t.triple === host);
    if (!hostTarget) fail(`host target ${host} is not in the supported target list`);
    targets = [hostTarget];
  }

  const artifacts: string[] = [];
  for (const target of targets) {
    const result = await buildTarget(
      target,
      version,
      args.releaseDir,
      args.verbose,
      args.skipExtras,
      host,
      extrasDir,
      args.deb,
    );
    if (result.archive) artifacts.push(result.archive);
    if (result.deb) artifacts.push(result.deb);
  }

  if (artifacts.length === 0) fail("no archives were produced");
  if (!args.skipChecksums) await writeChecksums(args.releaseDir, artifacts);

  // Clean up the shared extras directory unless we are in a partial build.
  if (extrasDir && args.all) {
    rmSync(extrasDir, { recursive: true, force: true });
  }

  log("done");
}

main().catch((err) => fail(err));
