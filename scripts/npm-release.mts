#!/usr/bin/env bun
/**
 * Build and publish @shiinasaku/hayate npm packages from a GitHub release.
 *
 * Usage:
 *   bun run scripts/npm-release.mts --dry-run          # build packages, skip publish
 *   bun run scripts/npm-release.mts --version 6.0.0    # explicit version
 *
 * In CI, the release-binaries workflow passes the GitHub release tag via
 * `--tag hayate@6.0.0`.
 */

import { existsSync, mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { createHash } from "node:crypto";

interface Target {
  triple: string;
  npmPkg: string;
  os: string;
  cpu: string;
  archive: "tar.gz" | "zip";
  binary: string;
}

const TARGETS: Target[] = [
  { triple: "x86_64-apple-darwin", npmPkg: "@shiinasaku/hayate-darwin-x64", os: "darwin", cpu: "x64", archive: "tar.gz", binary: "hayate" },
  { triple: "aarch64-apple-darwin", npmPkg: "@shiinasaku/hayate-darwin-arm64", os: "darwin", cpu: "arm64", archive: "tar.gz", binary: "hayate" },
  { triple: "x86_64-unknown-linux-gnu", npmPkg: "@shiinasaku/hayate-linux-x64", os: "linux", cpu: "x64", archive: "tar.gz", binary: "hayate" },
  { triple: "aarch64-unknown-linux-gnu", npmPkg: "@shiinasaku/hayate-linux-arm64", os: "linux", cpu: "arm64", archive: "tar.gz", binary: "hayate" },
  { triple: "x86_64-pc-windows-msvc", npmPkg: "@shiinasaku/hayate-win32-x64", os: "win32", cpu: "x64", archive: "zip", binary: "hayate.exe" },
  { triple: "aarch64-pc-windows-msvc", npmPkg: "@shiinasaku/hayate-win32-arm64", os: "win32", cpu: "arm64", archive: "zip", binary: "hayate.exe" },
  { triple: "x86_64-linux-android", npmPkg: "@shiinasaku/hayate-android-x64", os: "android", cpu: "x64", archive: "tar.gz", binary: "hayate" },
  { triple: "aarch64-linux-android", npmPkg: "@shiinasaku/hayate-android-arm64", os: "android", cpu: "arm64", archive: "tar.gz", binary: "hayate" },
];

interface Args {
  tag?: string;
  version?: string;
  releaseDir: string;
  dryRun: boolean;
  repo: string;
  skipDownload: boolean;
}

function parseArgs(): Args {
  const raw = process.argv.slice(2);
  const args: Args = {
    releaseDir: "npm/dist",
    dryRun: false,
    repo: "ShiinaSaku/Hayate",
    skipDownload: false,
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

    if (arg === "--tag") {
      args.tag = next();
    } else if (arg === "--version") {
      args.version = next();
    } else if (arg === "--release-dir" || arg === "-o") {
      args.releaseDir = next();
    } else if (arg === "--repo") {
      args.repo = next();
    } else if (arg === "--dry-run") {
      args.dryRun = true;
    } else if (arg === "--skip-download") {
      args.skipDownload = true;
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
  console.log(`npm release script for @shiinasaku/hayate

Usage: bun run scripts/npm-release.mts [options]

Options:
  --tag <tag>          GitHub release tag (e.g. hayate@6.0.0). Version is parsed from it.
  --version <version>  Override the version (skip Cargo.toml / tag parsing).
  --release-dir, -o    Output directory for built npm packages (default: npm/dist).
  --repo <owner/repo>  GitHub repository to download assets from (default: ShiinaSaku/Hayate).
  --dry-run            Build packages but do not publish to npm.
  --skip-download      Use existing archives in release-dir instead of downloading.
  --help, -h           Show this help.
`);
}

function log(...args: unknown[]) {
  console.log("[npm-release]", ...args);
}

function fail(...args: unknown[]): never {
  console.error("[npm-release] error:", ...args);
  process.exit(1);
}

async function readVersion(): Promise<string> {
  const cargoToml = await Bun.file(join(import.meta.dir, "..", "Cargo.toml")).text();
  const match = cargoToml.match(/^version\s*=\s*"([^"]+)"/m);
  if (!match?.[1]) fail("could not parse workspace version from Cargo.toml");
  return match[1];
}

function resolveVersion(args: Args): string {
  if (args.version) return args.version;
  if (args.tag) {
    const m = args.tag.match(/(?:^|@)(\d+\.\d+\.\d+(?:-[\w.]+)?)$/);
    if (m?.[1]) return m[1];
    fail(`could not parse version from tag ${args.tag}`);
  }
  fail("provide --version or --tag");
}

async function run(cmd: string, args: string[], options?: { cwd?: string; env?: Record<string, string | undefined> }): Promise<void> {
  const proc = Bun.spawn({
    cmd: [cmd, ...args],
    cwd: options?.cwd,
    env: { ...process.env, ...options?.env },
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdout, stderr] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text()]);
  const exit = await proc.exited;
  if (exit !== 0) {
    throw new Error(`${cmd} ${args.join(" ")} exited ${exit}\n${stdout}\n${stderr}`.trim());
  }
}

async function fetchArchive(url: string, dest: string): Promise<void> {
  log(`downloading ${url}`);
  const res = await fetch(url);
  if (!res.ok) fail(`failed to download ${url}: ${res.status} ${res.statusText}`);
  const buffer = await res.arrayBuffer();
  await Bun.write(dest, buffer);
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

async function extractArchive(archivePath: string, target: Target, outDir: string): Promise<void> {
  mkdirSync(outDir, { recursive: true });
  if (target.archive === "tar.gz") {
    await run("tar", ["-xzf", archivePath, "-C", outDir]);
  } else {
    await run("unzip", ["-q", archivePath, "-d", outDir]);
  }
}

function findBinary(outDir: string, binaryName: string): string | undefined {
  const candidates = [join(outDir, binaryName), join(outDir, ".", binaryName)];
  for (const c of candidates) {
    if (existsSync(c)) return c;
  }
  // Recurse one level for tar archives that contain ./
  for (const entry of readdirSync(outDir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      const c = join(outDir, entry.name, binaryName);
      if (existsSync(c)) return c;
    }
  }
  return undefined;
}

async function buildPlatformPackage(target: Target, version: string, releaseDir: string, archivePath: string): Promise<string> {
  const workDir = join(releaseDir, "work", target.npmPkg);
  rmSync(workDir, { recursive: true, force: true });
  mkdirSync(workDir, { recursive: true });

  await extractArchive(archivePath, target, workDir);

  const binPath = findBinary(workDir, target.binary);
  if (!binPath) fail(`could not find ${target.binary} in extracted archive for ${target.triple}`);

  const pkgDir = join(releaseDir, "packages", target.npmPkg);
  rmSync(pkgDir, { recursive: true, force: true });
  mkdirSync(pkgDir, { recursive: true });

  await Bun.write(join(pkgDir, target.binary), Bun.file(binPath));

  const pkgJson = {
    name: target.npmPkg,
    version,
    description: `Hayate CLI binary for ${target.os} ${target.cpu}`,
    license: "MIT",
    repository: { type: "git", url: "git+https://github.com/ShiinaSaku/Hayate.git" },
    homepage: "https://hayate.shiina.xyz",
    os: [target.os],
    cpu: [target.cpu],
    binary: target.binary,
    files: [target.binary],
  };
  writeFileSync(join(pkgDir, "package.json"), JSON.stringify(pkgJson, null, 2) + "\n", "utf8");

  rmSync(workDir, { recursive: true, force: true });
  return pkgDir;
}

async function buildMainPackage(version: string, releaseDir: string): Promise<string> {
  const pkgDir = join(releaseDir, "packages", "@shiinasaku/hayate");
  rmSync(pkgDir, { recursive: true, force: true });
  mkdirSync(pkgDir, { recursive: true });

  const baseJson = JSON.parse(await Bun.file(join(import.meta.dir, "..", "npm", "hayate", "package.json")).text());
  const optionalDependencies: Record<string, string> = {};
  for (const target of TARGETS) {
    optionalDependencies[target.npmPkg] = version;
  }
  baseJson.version = version;
  baseJson.optionalDependencies = optionalDependencies;
  delete baseJson["//"];

  writeFileSync(join(pkgDir, "package.json"), JSON.stringify(baseJson, null, 2) + "\n", "utf8");

  // Copy wrapper files.
  await Bun.write(join(pkgDir, "index.js"), Bun.file(join(import.meta.dir, "..", "npm", "hayate", "index.js")));
  mkdirSync(join(pkgDir, "bin"), { recursive: true });
  await Bun.write(join(pkgDir, "bin", "hayate.js"), Bun.file(join(import.meta.dir, "..", "npm", "hayate", "bin", "hayate.js")));
  await Bun.write(join(pkgDir, "README.md"), Bun.file(join(import.meta.dir, "..", "npm", "hayate", "README.md")));

  return pkgDir;
}

async function publishPackage(pkgDir: string, dryRun: boolean): Promise<void> {
  const pkgJson = JSON.parse(await Bun.file(join(pkgDir, "package.json")).text());
  log(`publishing ${pkgJson.name}@${pkgJson.version}`);
  if (dryRun) {
    log("  dry-run: skipping npm publish");
    return;
  }
  await run("npm", ["publish", "--access", "public"], { cwd: pkgDir });
}

async function main() {
  const args = parseArgs();
  const version = args.version ?? (args.tag ? resolveVersion(args) : await readVersion());
  const tag = args.tag ?? `hayate@${version}`;
  log(`version: ${version}`);
  log(`release tag: ${tag}`);

  const releaseDir = args.releaseDir;
  mkdirSync(releaseDir, { recursive: true });

  const archivesDir = join(releaseDir, "archives");
  mkdirSync(archivesDir, { recursive: true });

  if (!args.skipDownload) {
    for (const target of TARGETS) {
      const archiveName = `hayate-v${version}-${target.triple}.${target.archive}`;
      const archivePath = join(archivesDir, archiveName);
      if (!existsSync(archivePath)) {
        const url = `https://github.com/${args.repo}/releases/download/${tag}/${archiveName}`;
        await fetchArchive(url, archivePath);
      } else {
        log(`using cached ${archivePath}`);
      }
    }
  }

  const platformPkgDirs: string[] = [];
  for (const target of TARGETS) {
    const archiveName = `hayate-v${version}-${target.triple}.${target.archive}`;
    const archivePath = join(archivesDir, archiveName);
    if (!existsSync(archivePath)) fail(`archive not found: ${archivePath}`);
    const pkgDir = await buildPlatformPackage(target, version, releaseDir, archivePath);
    platformPkgDirs.push(pkgDir);
  }

  const mainPkgDir = await buildMainPackage(version, releaseDir);

  // Publish platform packages first, then the main package.
  for (const pkgDir of platformPkgDirs) {
    await publishPackage(pkgDir, args.dryRun);
  }
  await publishPackage(mainPkgDir, args.dryRun);

  log("done");
}

main().catch((err) => fail(err));
