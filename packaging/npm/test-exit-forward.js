#!/usr/bin/env node
"use strict";

/**
 * Smoke-test: npm bin must forward non-zero native exit codes without throwing.
 * Run: node packaging/npm/test-exit-forward.js
 */

const { spawnSync } = require("child_process");
const fs = require("fs");
const os = require("os");
const path = require("path");

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "codebeacon-npm-"));
const key = `${process.platform}-${process.arch}`;
const pkgName = {
  "linux-x64": "codebeacon-linux-x64",
  "linux-arm64": "codebeacon-linux-arm64",
  "darwin-x64": "codebeacon-darwin-x64",
  "darwin-arm64": "codebeacon-darwin-arm64",
  "win32-x64": "codebeacon-win32-x64",
}[key];

if (!pkgName) {
  console.error("skip: unsupported test platform", key);
  process.exit(0);
}

const pkgDir = path.join(tmp, "node_modules", pkgName);
fs.mkdirSync(pkgDir, { recursive: true });
fs.writeFileSync(
  path.join(pkgDir, "package.json"),
  JSON.stringify({ name: pkgName, version: "0.0.0", main: "codebeacon" })
);
const fakeBin = path.join(
  pkgDir,
  process.platform === "win32" ? "codebeacon.exe" : "codebeacon"
);
fs.writeFileSync(fakeBin, "#!/bin/sh\necho help-from-fake\nexit 2\n", {
  mode: 0o755,
});

const result = spawnSync(
  process.execPath,
  [path.join(__dirname, "bin", "codebeacon")],
  {
    encoding: "utf8",
    env: { ...process.env, NODE_PATH: path.join(tmp, "node_modules") },
  }
);
fs.rmSync(tmp, { recursive: true, force: true });

if (result.status !== 2) {
  console.error("expected exit 2, got", result.status);
  console.error("stdout:", result.stdout);
  console.error("stderr:", result.stderr);
  process.exit(1);
}
if (result.stderr && result.stderr.includes("Error: Command failed")) {
  console.error("wrapper threw Node stacktrace");
  process.exit(1);
}
if (!String(result.stdout).includes("help-from-fake")) {
  console.error("expected fake binary stdout");
  process.exit(1);
}

console.log("ok: exit code forwarded without Node stacktrace");
