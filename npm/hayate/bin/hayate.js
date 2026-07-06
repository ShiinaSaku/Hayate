#!/usr/bin/env node
import { spawn } from "node:child_process";
import { binaryPath } from "../index.js";

const bin = await binaryPath();
const child = spawn(bin, process.argv.slice(2), { stdio: "inherit" });
child.on("exit", (code) => process.exit(code ?? 1));
