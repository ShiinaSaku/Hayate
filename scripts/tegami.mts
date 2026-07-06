import { tegami } from "tegami";
import { runCli } from "tegami/cli";
import { cargo } from "tegami/plugins/cargo";
import { github } from "tegami/plugins/github";

const REPO = "ShiinaSaku/Hayate";

const paper = tegami({
  plugins: [
    github({
      repo: REPO,
      versionPr: {
        base: "master",
      },
    }),
    cargo(),
  ],
});

await runCli(paper);
