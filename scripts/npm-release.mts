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

import {
  chmodSync,
  existsSync,
  mkdirSync,
  readdirSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
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
  {
    triple: "x86_64-apple-darwin",
    npmPkg: "@shiinasaku/hayate-darwin-x64",
    os: "darwin",
    cpu: "x64",
    archive: "tar.gz",
    binary: "hayate",
  },
  {
    triple: "aarch64-apple-darwin",
    npmPkg: "@shiinasaku/hayate-darwin-arm64",
    os: "darwin",
    cpu: "arm64",
    archive: "tar.gz",
    binary: "hayate",
  },
  {
    triple: "x86_64-unknown-linux-gnu",
    npmPkg: "@shiinasaku/hayate-linux-x64",
    os: "linux",
    cpu: "x64",
    archive: "tar.gz",
    binary: "hayate",
  },
  {
    triple: "aarch64-unknown-linux-gnu",
    npmPkg: "@shiinasaku/hayate-linux-arm64",
    os: "linux",
    cpu: "arm64",
    archive: "tar.gz",
    binary: "hayate",
  },
  {
    triple: "x86_64-pc-windows-msvc",
    npmPkg: "@shiinasaku/hayate-win32-x64",
    os: "win32",
    cpu: "x64",
    archive: "zip",
    binary: "hayate.exe",
  },
  {
    triple: "aarch64-pc-windows-msvc",
    npmPkg: "@shiinasaku/hayate-win32-arm64",
    os: "win32",
    cpu: "arm64",
    archive: "zip",
    binary: "hayate.exe",
  },
  {
    triple: "x86_64-linux-android",
    npmPkg: "@shiinasaku/hayate-android-x64",
    os: "android",
    cpu: "x64",
    archive: "tar.gz",
    binary: "hayate",
  },
  {
    triple: "aarch64-linux-android",
    npmPkg: "@shiinasaku/hayate-android-arm64",
    os: "android",
    cpu: "arm64",
    archive: "tar.gz",
    binary: "hayate",
  },
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

async function run(
  cmd: string,
  args: string[],
  options?: { cwd?: string; env?: Record<string, string | undefined> },
): Promise<string> {
  const proc = Bun.spawn({
    cmd: [cmd, ...args],
    cwd: options?.cwd,
    env: { ...process.env, ...options?.env },
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdout, stderr] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
  ]);
  const exit = await proc.exited;
  if (exit !== 0) {
    throw new Error(`${cmd} ${args.join(" ")} exited ${exit}\n${stdout}\n${stderr}`.trim());
  }
  return stdout;
}

/** Downloads to a temp path then renames, so a killed run never leaves a
 * half-written archive that a later `--skip-download` run would trust. */
async function fetchArchive(url: string, dest: string): Promise<void> {
  log(`downloading ${url}`);
  const res = await fetch(url);
  if (!res.ok) fail(`failed to download ${url}: ${res.status} ${res.statusText}`);
  const buffer = await res.arrayBuffer();
  const tmp = `${dest}.tmp-${process.pid}`;
  await Bun.write(tmp, buffer);
  renameSync(tmp, dest);
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

/** Rejects absolute paths and `..` traversal before extraction — downloaded
 * archives are remote input and must not write outside `outDir`. */
function assertSafeArchiveEntries(entries: string[], archivePath: string): void {
  for (const entry of entries) {
    const normalized = entry.replace(/\\/g, "/");
    if (
      normalized.startsWith("/") ||
      /^[A-Za-z]:\//.test(normalized) ||
      normalized.split("/").includes("..")
    ) {
      fail(`unsafe entry ${JSON.stringify(entry)} in ${archivePath}`);
    }
  }
}

async function listArchiveEntries(archivePath: string, target: Target): Promise<string[]> {
  const output =
    target.archive === "tar.gz"
      ? await run("tar", ["-tzf", archivePath])
      : await run("unzip", ["-Z1", archivePath]);
  return output
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

async function extractArchive(archivePath: string, target: Target, outDir: string): Promise<void> {
  assertSafeArchiveEntries(await listArchiveEntries(archivePath, target), archivePath);
  mkdirSync(outDir, { recursive: true });
  if (target.archive === "tar.gz") {
    await run("tar", ["-xzf", archivePath, "-C", outDir]);
  } else {
    await run("unzip", ["-q", archivePath, "-d", outDir]);
  }
}

/** Parses a `sha256sum`-format manifest (`<hex>  <name>` per line). */
async function readChecksumManifest(path: string): Promise<Map<string, string>> {
  const text = await Bun.file(path).text();
  const sums = new Map<string, string>();
  for (const line of text.split("\n")) {
    const m = line.match(/^([0-9a-f]{64})\s+\*?(.+)$/);
    if (m?.[1] && m[2]) sums.set(m[2].trim(), m[1]);
  }
  if (sums.size === 0) fail(`no checksums parsed from ${path}`);
  return sums;
}

async function fetchChecksumManifest(
  repo: string,
  tag: string,
  archivesDir: string,
): Promise<Map<string, string>> {
  const dest = join(archivesDir, "SHA256SUMS.txt");
  await fetchArchive(`https://github.com/${repo}/releases/download/${tag}/SHA256SUMS.txt`, dest);
  return readChecksumManifest(dest);
}

async function verifyArchiveChecksum(
  archivePath: string,
  archiveName: string,
  sums: Map<string, string>,
): Promise<void> {
  const expected = sums.get(archiveName);
  if (!expected) fail(`no checksum for ${archiveName} in SHA256SUMS.txt`);
  const actual = await sha256(archivePath);
  if (actual !== expected) {
    fail(`checksum mismatch for ${archiveName}: expected ${expected}, got ${actual}`);
  }
  log(`verified ${archiveName}`);
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

async function buildPlatformPackage(
  target: Target,
  version: string,
  releaseDir: string,
  archivePath: string,
): Promise<string> {
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
  // Bun.write does not preserve the source file's executable bit, so set it
  // explicitly; without this the npm-installed binary fails with EACCES on
  // Unix/Android.
  if (target.os !== "win32") {
    chmodSync(join(pkgDir, target.binary), 0o755);
  }

  const template = JSON.parse(
    await Bun.file(join(import.meta.dir, "..", "npm", "pkg-template", "package.json")).text(),
  ) as Record<string, unknown>;
  const pkgJson = {
    ...template,
    name: target.npmPkg,
    version,
    description: `Hayate CLI binary for ${target.os} ${target.cpu}`,
    os: [target.os],
    cpu: [target.cpu],
    binary: target.binary,
    files: [target.binary],
  };
  writeFileSync(join(pkgDir, "package.json"), JSON.stringify(pkgJson, null, 2) + "\n", "utf8");

  rmSync(workDir, { recursive: true, force: true });
  return pkgDir;
}

/** Bundles the npm wrapper (TypeScript sources → dist) via tsdown. Requires
 * Node >= 22.18 at build time; the emitted output still targets Node 18. */
async function buildWrapperDist(root: string): Promise<void> {
  if (!existsSync(join(root, "node_modules", ".bin", "tsdown")))
    fail("tsdown not installed — run `bun install` first");
  await run("bun", ["run", "npm:build"], { cwd: root });
}

async function buildMainPackage(version: string, releaseDir: string): Promise<string> {
  const root = join(import.meta.dir, "..");
  await buildWrapperDist(root);

  const pkgDir = join(releaseDir, "packages", "@shiinasaku/hayate");
  rmSync(pkgDir, { recursive: true, force: true });
  mkdirSync(pkgDir, { recursive: true });

  const baseJson = JSON.parse(
    await Bun.file(join(import.meta.dir, "..", "npm", "hayate", "package.json")).text(),
  );
  const optionalDependencies: Record<string, string> = {};
  for (const target of TARGETS) {
    optionalDependencies[target.npmPkg] = version;
  }
  baseJson.version = version;
  baseJson.optionalDependencies = optionalDependencies;
  delete baseJson["//"];
  delete baseJson.scripts; // build tooling must not ship in the published package

  writeFileSync(join(pkgDir, "package.json"), JSON.stringify(baseJson, null, 2) + "\n", "utf8");

  // Copy wrapper files (tsdown output; layout kept: index.js + bin/hayate.js).
  const wrapperDist = join(root, "npm", "hayate", "dist");
  await Bun.write(join(pkgDir, "index.js"), Bun.file(join(wrapperDist, "index.js")));
  await Bun.write(join(pkgDir, "index.d.ts"), Bun.file(join(wrapperDist, "index.d.ts")));
  mkdirSync(join(pkgDir, "bin"), { recursive: true });
  await Bun.write(
    join(pkgDir, "bin", "hayate.js"),
    Bun.file(join(wrapperDist, "bin", "hayate.js")),
  );
  await Bun.write(join(pkgDir, "README.md"), Bun.file(join(root, "npm", "hayate", "README.md")));

  return pkgDir;
}

async function publishPackage(pkgDir: string, dryRun: boolean): Promise<void> {
  const pkgJson = JSON.parse(await Bun.file(join(pkgDir, "package.json")).text());
  log(`publishing ${pkgJson.name}@${pkgJson.version}`);
  if (dryRun) {
    log("  dry-run: skipping npm publish");
    return;
  }
  const publishArgs = ["publish", "--access", "public"];
  // Provenance attestation when publishing from GitHub Actions (OIDC).
  if (process.env.GITHUB_ACTIONS === "true") publishArgs.push("--provenance");
  await run("npm", publishArgs, { cwd: pkgDir });
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
    const sums = await fetchChecksumManifest(args.repo, tag, archivesDir);
    for (const target of TARGETS) {
      const archiveName = `hayate-v${version}-${target.triple}.${target.archive}`;
      const archivePath = join(archivesDir, archiveName);
      if (!existsSync(archivePath)) {
        const url = `https://github.com/${args.repo}/releases/download/${tag}/${archiveName}`;
        await fetchArchive(url, archivePath);
      } else {
        log(`using cached ${archivePath}`);
      }
      await verifyArchiveChecksum(archivePath, archiveName, sums);
    }
  }

  const platformPkgDirs: string[] = [];
  for (const target of TARGETS) {
    const archiveName = `hayate-v${version}-${target.triple}.${target.archive}`;
    const archivePath = join(archivesDir, archiveName);
    if (!existsSync(archivePath)) fail(`archive not found: ${archivePath}`);
    if (args.skipDownload) {
      // With pre-seeded archives, verify against a local manifest if one was
      // provided alongside them.
      const sumsPath = join(archivesDir, "SHA256SUMS.txt");
      if (existsSync(sumsPath)) {
        await verifyArchiveChecksum(archivePath, archiveName, await readChecksumManifest(sumsPath));
      } else {
        log(`skip-download: no SHA256SUMS.txt for ${archiveName}, trusting local file`);
      }
    }
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
