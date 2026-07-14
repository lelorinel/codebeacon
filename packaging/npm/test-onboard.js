#!/usr/bin/env node
"use strict";

/**
 * Unit tests for npx onboarding helpers.
 * Run: node packaging/npm/test-onboard.js
 */

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const onboard = require("./lib/onboard");

function test(name, fn) {
  try {
    fn();
    console.log(`ok: ${name}`);
  } catch (err) {
    console.error(`FAIL: ${name}`);
    throw err;
  }
}

test("majorMinor strips patch", () => {
  assert.strictEqual(onboard.majorMinor("0.5.0"), "0.5");
  assert.strictEqual(onboard.majorMinor("0.5.1"), "0.5");
  assert.strictEqual(onboard.majorMinor("1.2.3"), "1.2");
  assert.strictEqual(onboard.majorMinor("v0.6.0"), "0.6");
});

test("isEligibleCommand", () => {
  assert.strictEqual(onboard.isEligibleCommand([]), true);
  assert.strictEqual(onboard.isEligibleCommand(["help"]), true);
  assert.strictEqual(onboard.isEligibleCommand(["--help"]), true);
  assert.strictEqual(onboard.isEligibleCommand(["-h"]), true);
  assert.strictEqual(onboard.isEligibleCommand(["init"]), true);
  assert.strictEqual(onboard.isEligibleCommand(["init", "--root", "."]), true);
  assert.strictEqual(onboard.isEligibleCommand(["serve"]), false);
  assert.strictEqual(onboard.isEligibleCommand(["query", "foo"]), false);
  assert.strictEqual(onboard.isEligibleCommand(["install"]), false);
});

test("shouldOnboard gates", () => {
  const base = {
    argv: [],
    stdinIsTTY: true,
    stdoutIsTTY: true,
    packageMinor: "0.5",
    env: {},
  };
  assert.strictEqual(onboard.shouldOnboard(base), true);
  assert.strictEqual(
    onboard.shouldOnboard({ ...base, argv: ["serve"] }),
    false
  );
  assert.strictEqual(
    onboard.shouldOnboard({ ...base, stdinIsTTY: false }),
    false
  );
  assert.strictEqual(
    onboard.shouldOnboard({
      ...base,
      env: { CODEBEACON_SKIP_ONBOARD: "1" },
    }),
    false
  );
  assert.strictEqual(
    onboard.shouldOnboard({ ...base, dismissedMinor: "0.5" }),
    false
  );
  assert.strictEqual(
    onboard.shouldOnboard({ ...base, dismissedMinor: "0.5", packageMinor: "0.6" }),
    true
  );
});

test("choiceFromInput", () => {
  assert.strictEqual(onboard.choiceFromInput("1"), "alias");
  assert.strictEqual(onboard.choiceFromInput("2"), "path");
  assert.strictEqual(onboard.choiceFromInput("3"), "npx");
  assert.strictEqual(onboard.choiceFromInput(""), "npx");
  assert.strictEqual(onboard.choiceFromInput("x"), null);
});

test("zsh alias rc write + replace", () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "cb-onboard-"));
  const zshrc = path.join(home, ".zshrc");
  fs.writeFileSync(zshrc, "# existing\n");

  let r = onboard.applyShellChoice("alias", "zsh", "/opt/bin", home);
  assert.strictEqual(r.ok, true);
  let text = fs.readFileSync(zshrc, "utf8");
  assert.ok(text.includes(onboard.MARKER_START));
  assert.ok(text.includes("alias codebeacon='npx codebeacon'"));
  assert.ok(text.includes("# existing"));

  r = onboard.applyShellChoice("path", "zsh", "/opt/bin", home);
  assert.strictEqual(r.ok, true);
  text = fs.readFileSync(zshrc, "utf8");
  assert.ok(text.includes('export PATH="/opt/bin:$PATH"'));
  assert.strictEqual(text.split(onboard.MARKER_START).length - 1, 1);

  fs.rmSync(home, { recursive: true, force: true });
});

test("fish path write", () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "cb-onboard-"));
  const r = onboard.applyShellChoice(
    "path",
    "fish",
    "/opt/codebeacon",
    home
  );
  assert.strictEqual(r.ok, true);
  const text = fs.readFileSync(r.file, "utf8");
  assert.ok(text.includes("fish_add_path /opt/codebeacon"));
  fs.rmSync(home, { recursive: true, force: true });
});

test("unknown shell returns manual snippet", () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "cb-onboard-"));
  const r = onboard.applyShellChoice("alias", "unknown", "/x", home);
  assert.strictEqual(r.ok, false);
  assert.ok(r.manual.includes("alias codebeacon"));
  fs.rmSync(home, { recursive: true, force: true });
});

test("state read/write", () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "cb-onboard-"));
  onboard.writeState(home, {
    dismissedMinor: "0.5",
    choice: "npx",
    shell: "zsh",
    updatedAt: "2026-07-14T00:00:00.000Z",
  });
  const s = onboard.readState(home);
  assert.strictEqual(s.dismissedMinor, "0.5");
  assert.strictEqual(s.choice, "npx");
  fs.rmSync(home, { recursive: true, force: true });
});

console.log("\nall onboard tests passed");
