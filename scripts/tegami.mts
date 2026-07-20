import { tegami } from "tegami";
import { runCli } from "tegami/cli";
import { cargo } from "tegami/plugins/cargo";
import { github } from "tegami/plugins/github";

const REPO = "ShiinaSaku/Hayate";

/**
 * Release codenames by major version. v6 is "Shinka" (進化 — evolution).
 * Add the next major's codename here when the time comes.
 */
const CODENAMES: Record<string, string> = {
  "6": "Shinka (進化)",
};

/** GitHub release title, e.g. `Hayate v6.0.0 — Shinka (進化)`. */
function releaseTitle(version: string): string {
  const codename = CODENAMES[version.split(".")[0] ?? ""];
  return codename ? `Hayate v${version} — ${codename}` : `hayate@${version}`;
}

const paper = tegami({
  // Both workspace crates share one [workspace.package] version, so they must
  // bump in lockstep (`syncBump`) and ship under a single git tag + GitHub
  // release (`syncGitTag`) — otherwise tegami bumps the workspace version
  // once per crate and the releases race each other.
  groups: {
    hayate: { syncBump: true, syncGitTag: true },
  },
  packages: {
    hayate: { group: "hayate" },
    "hayate-cli": { group: "hayate" },
  },
  plugins: [
    github({
      repo: REPO,
      versionPr: {
        base: "master",
      },
      release: {
        create({ pkg }) {
          return { title: releaseTitle(pkg.version ?? "0.0.0") };
        },
        createGrouped({ tag }) {
          return { title: releaseTitle(tag.split("@").pop() ?? tag) };
        },
      },
    }),
    cargo(),
  ],
});

await runCli(paper);
