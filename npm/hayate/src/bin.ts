#!/usr/bin/env node
import { spawn } from "node:child_process";

import { binaryPath } from "./index.js";

// Unsupported platforms land here — print the one-line reason, not a stack.
const bin = await binaryPath().catch((err: unknown) => {
  console.error(err instanceof Error ? err.message : String(err));
  process.exit(1);
});
const child = spawn(bin, process.argv.slice(2), { stdio: "inherit" });
child.on("exit", (code) => process.exit(code ?? 1));
