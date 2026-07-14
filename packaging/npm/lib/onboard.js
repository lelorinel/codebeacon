"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const readline = require("readline");

const MARKER_START = "# codebeacon-onboard-start";
const MARKER_END = "# codebeacon-onboard-end";

const ELIGIBLE_HELP = new Set(["help", "--help", "-h"]);

/**
 * @param {string} version semver string e.g. "0.5.0"
 * @returns {string} "0.5"
 */
function majorMinor(version) {
  const parts = String(version).replace(/^v/, "").split(".");
  const major = parts[0] || "0";
  const minor = parts[1] || "0";
  return `${major}.${minor}`;
}

/**
 * @param {string[]} argv process.argv.slice(2)
 */
function isEligibleCommand(argv) {
  if (!argv || argv.length === 0) return true;
  const cmd = argv[0];
  if (ELIGIBLE_HELP.has(cmd)) return true;
  if (cmd === "init") return true;
  return false;
}

/**
 * @param {{
 *   argv: string[],
 *   env?: NodeJS.ProcessEnv,
 *   stdinIsTTY?: boolean,
 *   stdoutIsTTY?: boolean,
 *   dismissedMinor?: string | null,
 *   packageMinor?: string,
 * }} opts
 */
function shouldOnboard(opts) {
  const env = opts.env || process.env;
  if (env.CODEBEACON_SKIP_ONBOARD) return false;
  if (!opts.stdinIsTTY || !opts.stdoutIsTTY) return false;
  if (!isEligibleCommand(opts.argv || [])) return false;
  const packageMinor = opts.packageMinor;
  if (
    packageMinor &&
    opts.dismissedMinor != null &&
    opts.dismissedMinor === packageMinor
  ) {
    return false;
  }
  return true;
}

function detectShell(env) {
  const shellPath = (env && env.SHELL) || "";
  const base = path.basename(shellPath).toLowerCase();
  if (base === "bash" || base === "zsh" || base === "fish") return base;
  return "unknown";
}

function statePath(homedir) {
  return path.join(homedir, ".config", "codebeacon", "onboarding.json");
}

function readState(homedir, readFileSync = fs.readFileSync) {
  const p = statePath(homedir);
  try {
    return JSON.parse(readFileSync(p, "utf8"));
  } catch {
    return null;
  }
}

function writeState(homedir, state, deps = {}) {
  const writeFileSync = deps.writeFileSync || fs.writeFileSync;
  const mkdirSync = deps.mkdirSync || fs.mkdirSync;
  const p = statePath(homedir);
  mkdirSync(path.dirname(p), { recursive: true });
  writeFileSync(p, JSON.stringify(state, null, 2) + "\n");
}

function wrapMarked(body) {
  return `${MARKER_START}\n${body.trimEnd()}\n${MARKER_END}\n`;
}

function replaceOrAppendMarked(existing, block) {
  const start = existing.indexOf(MARKER_START);
  const end = existing.indexOf(MARKER_END);
  if (start !== -1 && end !== -1 && end > start) {
    const afterEnd = end + MARKER_END.length;
    const before = existing.slice(0, start);
    let after = existing.slice(afterEnd);
    if (after.startsWith("\n")) after = after.slice(1);
    return before + block + after;
  }
  const sep =
    existing.length === 0 || existing.endsWith("\n") ? "" : "\n";
  return existing + sep + (existing.length ? "\n" : "") + block;
}

function aliasSnippet(shell) {
  if (shell === "fish") {
    return "alias codebeacon='npx codebeacon'";
  }
  return "alias codebeacon='npx codebeacon'";
}

function pathSnippet(shell, binDir) {
  if (shell === "fish") {
    return `fish_add_path ${binDir}`;
  }
  // Escape for shell double-quotes: backslash and quote
  const escaped = binDir.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  return `export PATH="${escaped}:$PATH"`;
}

function rcTarget(shell, homedir) {
  if (shell === "zsh") {
    return {
      file: path.join(homedir, ".zshrc"),
      kind: "rc",
    };
  }
  if (shell === "bash") {
    return {
      file: path.join(homedir, ".bashrc"),
      kind: "rc",
    };
  }
  if (shell === "fish") {
    return {
      file: path.join(homedir, ".config", "fish", "conf.d", "codebeacon.fish"),
      kind: "fish-conf",
    };
  }
  return null;
}

/**
 * Apply alias or path choice to shell config.
 * @returns {{ ok: boolean, file?: string, manual?: string, error?: string }}
 */
function applyShellChoice(choice, shell, binDir, homedir, deps = {}) {
  const readFileSync = deps.readFileSync || fs.readFileSync;
  const writeFileSync = deps.writeFileSync || fs.writeFileSync;
  const mkdirSync = deps.mkdirSync || fs.mkdirSync;

  let body;
  if (choice === "alias") {
    body = aliasSnippet(shell);
  } else if (choice === "path") {
    body = pathSnippet(shell, binDir);
  } else {
    return { ok: true };
  }

  const target = rcTarget(shell, homedir);
  const block = wrapMarked(body);

  if (!target) {
    return {
      ok: false,
      manual: block,
      error: `Unknown shell; add this manually:\n${block}`,
    };
  }

  try {
    mkdirSync(path.dirname(target.file), { recursive: true });
    let existing = "";
    try {
      existing = readFileSync(target.file, "utf8");
    } catch {
      existing = "";
    }
    writeFileSync(target.file, replaceOrAppendMarked(existing, block));
    return { ok: true, file: target.file };
  } catch (err) {
    return {
      ok: false,
      file: target.file,
      manual: block,
      error: err.message || String(err),
    };
  }
}

function choiceFromInput(raw) {
  const t = String(raw || "").trim();
  if (t === "1") return "alias";
  if (t === "2") return "path";
  if (t === "3" || t === "") return "npx";
  return null;
}

function askChoice(stdin, stderr) {
  return new Promise((resolve) => {
    const rl = readline.createInterface({ input: stdin, output: stderr });
    const prompt =
      "\nCodebeacon (npx) — how do you want to run it in your terminal?\n" +
      "  1) Quick alias     → codebeacon ≡ npx codebeacon (per shell startup)\n" +
      "  2) Permanent PATH  → add the native binary directory to PATH\n" +
      "  3) Stick with npx  → no shell changes; use: npx codebeacon <cmd>\n" +
      "Choice [1/2/3]: ";
    const ask = (attempt) => {
      rl.question(prompt, (answer) => {
        const choice = choiceFromInput(answer);
        if (choice) {
          rl.close();
          resolve(choice);
          return;
        }
        if (attempt >= 1) {
          rl.close();
          resolve("npx");
          return;
        }
        stderr.write("Invalid choice. Try again.\n");
        ask(attempt + 1);
      });
    };
    ask(0);
  });
}

/**
 * Run interactive onboarding then persist state.
 * @param {{
 *   binPath: string,
 *   version: string,
 *   env?: NodeJS.ProcessEnv,
 *   homedir?: string,
 *   stdin?: NodeJS.ReadableStream,
 *   stderr?: NodeJS.WritableStream,
 *   ask?: () => Promise<"alias"|"path"|"npx">,
 * }} opts
 */
async function runOnboard(opts) {
  const env = opts.env || process.env;
  const homedir = opts.homedir || os.homedir();
  const stderr = opts.stderr || process.stderr;
  const stdin = opts.stdin || process.stdin;
  const shell = detectShell(env);
  const packageMinor = majorMinor(opts.version);
  const binDir = path.dirname(opts.binPath);

  const choice =
    (opts.ask && (await opts.ask())) ||
    (await askChoice(stdin, stderr));

  const applied = applyShellChoice(choice, shell, binDir, homedir);

  if (choice === "npx") {
    stderr.write("\nNo shell changes. Use: npx codebeacon <cmd>\n");
  } else if (applied.ok && applied.file) {
    stderr.write(`\nUpdated ${applied.file}\n`);
    stderr.write("Open a new shell (or source that file) to use `codebeacon`.\n");
  } else if (!applied.ok) {
    stderr.write(`\nCould not update shell config: ${applied.error}\n`);
    if (applied.manual) {
      stderr.write(applied.manual);
    }
  }

  writeState(homedir, {
    dismissedMinor: packageMinor,
    choice,
    shell,
    updatedAt: new Date().toISOString(),
  });

  return { choice, shell, applied };
}

module.exports = {
  MARKER_START,
  MARKER_END,
  majorMinor,
  isEligibleCommand,
  shouldOnboard,
  detectShell,
  statePath,
  readState,
  writeState,
  wrapMarked,
  replaceOrAppendMarked,
  aliasSnippet,
  pathSnippet,
  rcTarget,
  applyShellChoice,
  choiceFromInput,
  runOnboard,
};
