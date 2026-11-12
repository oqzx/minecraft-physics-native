// The release validation matrix (B292): every job a release candidate must pass, run on the current HEAD, with a retained
// record of what ran, on what, and what it counted.
//
//   node scripts/release-matrix.cjs                          the whole matrix on all 50 releases
//   node scripts/release-matrix.cjs --jobs a,b --releases x,y   a subset (its record says so, and it is never green)
//   node scripts/release-matrix.cjs --resume [record.json]   go on with the latest record of HEAD, keeping its passed units
//                                                            (--rerun runs the asked-for units again, passed or not)
//   node scripts/release-matrix.cjs --list                   the jobs and their commands
//   node scripts/release-matrix.cjs --doc [record.json]      writes docs/release-matrix.md from a record
//
// --corpus <dir> is the pinned recordings (default REPLAY_RECORDINGS, else E:/mc-physics-demo-wt/corpus); --work <dir> keeps
// the logs and scratch files (default the OS temp folder). The record is audit/release-matrix/<commit>-<date>.json and is
// written after every unit, so a stopped run resumes where it stopped.
//
// A unit passes only when its command exits 0, its summary was read, it selected and compared at least one case, and it
// failed none. A run with an entry in the recorder's known-failures.json (or no recorder to read it from), a dirty tree,
// units of more than one commit or any required unit missing is red (`evaluate`). Nothing that did not run, or ran on
// nothing, counts as passing.
//
// The replay jobs run in the recorder package (@minecraft-physics-native/recorder), which is not part of this repository:
// RECORDER_DIR, else the workspace's recorder beside this checkout. Its corpus manifests and known failures are read there.
"use strict";
const { spawn, execFileSync } = require("node:child_process");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const recordDirectory = path.join(root, "audit", "release-matrix");
const docPath = path.join(root, "docs", "release-matrix.md");
/** The recorder package the replay jobs run in. */
const recorderRoot = path.resolve(process.env.RECORDER_DIR ?? path.join(root, "..", "recorder"));

/** The releases a release candidate covers: those the engine registers, in order. */
function requestedReleases() {
  const { registeredReleases } = require("./registered-releases.cjs");
  return registeredReleases().map((entry) => entry.version);
}

const number = (text, pattern) => {
  const match = pattern.exec(text);
  return match ? Number(match[1].replaceAll(",", "")) : undefined;
};
const lastMatch = (text, pattern) => {
  let found;
  for (const match of text.matchAll(pattern)) found = match;
  return found;
};

// ---- parsers: each reads one command's output into counts; `undefined` counts mean the summary was not found ----

/** cargo test: the sum of every `test result` line. */
function parseCargo(text) {
  const lines = [...text.matchAll(/^test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored/gm)];
  if (lines.length === 0) return undefined;
  const sum = (index) => lines.reduce((total, line) => total + Number(line[index]), 0);
  return { selected: sum(2) + sum(3) + sum(4), compared: sum(2) + sum(3), passed: sum(2), failed: sum(3), skipped: sum(4) };
}

/** node --test: its closing tallies. */
function parseNodeTest(text) {
  const tests = number(text, /^ℹ tests (\d+)/m);
  if (tests === undefined) return undefined;
  const passed = number(text, /^ℹ pass (\d+)/m) ?? 0;
  const failed = (number(text, /^ℹ fail (\d+)/m) ?? 0) + (number(text, /^ℹ cancelled (\d+)/m) ?? 0);
  const skipped = (number(text, /^ℹ skipped (\d+)/m) ?? 0) + (number(text, /^ℹ todo (\d+)/m) ?? 0);
  return { selected: tests, compared: passed + failed, passed, failed, skipped };
}

/** The recorder's replay/cli.ts, the player's replay: `N recording(s), stepwise[ with phases], a matching, b known, ...`. */
function parseReplay(text) {
  const line = lastMatch(text, /^(\d+) recording\(s\), (?:stepwise|continuous)(?: with phases)?, (\d+) matching, (\d+) known, (\d+) unsupported, (\d+) untested, (\d+) unexpected/gm);
  if (!line) return undefined;
  const [, recordings, matching, known, unsupported, untested, unexpected] = line.map(Number);
  return { selected: recordings, compared: matching + known + unexpected, passed: matching, known, unsupported, untested, failed: unexpected };
}

/** cli.ts --moving-only: one line per recording and side, `ok`, `none` or `FAIL ` with `[moving <side>]`. */
function parseMoving(text) {
  const recordings = number(text, /^(\d+) recording\(s\),/m);
  if (recordings === undefined) return undefined;
  const count = (tag) => [...text.matchAll(new RegExp(`^${tag}\\s+\\S+ \\[moving `, "gm"))].length;
  const bad = [...text.matchAll(/^BAD /gm)].length;
  const passed = count("ok");
  const failed = count("FAIL") + bad;
  return { selected: recordings, compared: passed + failed, passed, untested: count("none"), failed };
}

/**
 * cli.ts --projectiles: one line per recording, `ok`, `none` (no projectile in it), `FAIL ` or `BAD `. The mode's own total
 * counts a recording with no projectile as unexpected and exits 1 for it, so the lines decide here.
 */
function parseProjectiles(text) {
  const recordings = number(text, /^(\d+) recording\(s\), \d+ matching, \d+ unexpected$/m);
  if (recordings === undefined) return undefined;
  const count = (tag) => [...text.matchAll(new RegExp(`^${tag}\\s`, "gm"))].length;
  const passed = count("ok");
  const failed = count("FAIL") + count("BAD");
  return { selected: recordings, compared: passed + failed, passed, untested: count("none"), failed };
}

/** cli.ts --adapter: `N recording(s), M matching, X with no decision to compare, U unexpected`. */
function parseAdapter(text) {
  const line = lastMatch(text, /^(\d+) recording\(s\), (\d+) matching, (\d+) with no decision to compare, (\d+) unexpected/gm);
  if (!line) return undefined;
  const [, recordings, matching, none, unexpected] = line.map(Number);
  return { selected: recordings, compared: matching + unexpected, passed: matching, untested: none, failed: unexpected };
}

/** cli.ts --bot-carts: `N recording(s), M matching, X with no ridden cart, B unexpected`. */
function parseBotCarts(text) {
  const line = lastMatch(text, /^(\d+) recording\(s\), (\d+) matching, (\d+) with no ridden cart, (\d+) unexpected/gm);
  if (!line) return undefined;
  const [, recordings, matching, none, unexpected] = line.map(Number);
  return { selected: recordings, compared: matching + unexpected, passed: matching, untested: none, failed: unexpected };
}

/** cli.ts --bot: `N recording(s), bot: a matching, b unsupported, c past the hands, d codec, e different`. */
function parseBot(text) {
  const line = lastMatch(text, /^(\d+) recording\(s\), bot: (\d+) matching, (\d+) unsupported, (\d+) past the hands, (\d+) codec, (\d+) different/gm);
  if (!line) return undefined;
  const [, recordings, matching, unsupported, hands, codec, different] = line.map(Number);
  // (past the hands and a codec stop are cases the bot did not finish: they are no match)
  return { selected: recordings, compared: matching + hands + codec + different, passed: matching, unsupported, failed: hands + codec + different };
}

/** conformance-cli.ts check: the strict gate of the release over its pinned corpus. */
function parseConformance(text, { json }) {
  if (!json || !fs.existsSync(json)) return undefined;
  const report = JSON.parse(fs.readFileSync(json, "utf8"));
  const outcomes = report.outcomes ?? {};
  const continuous = (report.evidence ?? []).find((row) => row.category === "continuous") ?? {};
  return {
    selected: report.corpus?.selected ?? 0,
    compared: (outcomes.compared ?? 0) + (outcomes.unexpected ?? 0) + (outcomes.known ?? 0),
    passed: outcomes.compared ?? 0,
    known: outcomes.known ?? 0,
    unsupported: outcomes.unsupported ?? 0,
    failed: (outcomes.unexpected ?? 0) + (report.status === "complete" ? 0 : 1),
    continuousPassed: continuous.passed ?? 0,
    continuousFailed: continuous.failed ?? 0,
    status: report.status,
    blockers: (report.blockers ?? []).length,
  };
}

/** random-run.ts: `seed S: N generated, a compared, b different, c invalid, d unsupported, e nondeterministic, f missing`. */
function parseFuzz(text) {
  const line = lastMatch(text, /^seed \S+: (\d+) generated, (.*)$/gm);
  if (!line) return undefined;
  const tally = Object.fromEntries([...line[2].matchAll(/(\d+) (\w+)/g)].map((match) => [match[2], Number(match[1])]));
  const failed = (tally.different ?? 0) + (tally.nondeterministic ?? 0) + (tally.invalid ?? 0) + (tally.missing ?? 0);
  return { selected: Number(line[1]), compared: (tally.compared ?? 0) + (tally.different ?? 0), passed: tally.compared ?? 0, unsupported: tally.unsupported ?? 0, failed };
}

/** scripts/determinism/compare.cjs between two replays of one build: `... N runs, T ticks, E runs ended by ..., D differences`. */
function parseDeterminism(text) {
  const line = lastMatch(text, /: (\d+) runs, (\d+) ticks, (\d+) runs ended by the same refusal, (\d+) differences$/gm);
  if (!line) return undefined;
  const [, runs, ticks, , differences] = line.map(Number);
  return { selected: runs, compared: runs, passed: runs - differences, failed: differences, ticks };
}

/**
 * scripts/bench/run.cjs against the acceptance in audit/bench-acceptance.json (B286): each cell of a release, scene and
 * timed mode is compared with the baseline's. A cell fails when its outcome digest differs (unless accepted), when its
 * minimum CPU time exceeds maxRatio times the baseline's, or when the baseline has no such cell; every cell fails on
 * another machine, where the baseline says nothing. The numbers are kept in the record as they are.
 */
function parseBench(text, { json }) {
  if (!json || !fs.existsSync(json)) return undefined;
  const report = JSON.parse(fs.readFileSync(json, "utf8"));
  const acceptance = JSON.parse(fs.readFileSync(path.join(root, "audit", "bench-acceptance.json"), "utf8"));
  const verdict = benchVerdict(report, acceptance, JSON.parse(fs.readFileSync(path.join(root, acceptance.baseline), "utf8")));
  return { ...verdict, numbers: { suite: report.suite, identity: report.identity, releases: report.releases } };
}

/** The acceptance's judgement of a bench report: counts over its cells, the problems, and the releases it covered. */
function benchVerdict(report, acceptance, baseline) {
  const problems = [];
  const sameMachine = report.identity?.platform === acceptance.machine.platform && report.identity?.cpu === acceptance.machine.cpu;
  if (!sameMachine) problems.push(`measured on ${report.identity?.cpu?.trim()} ${report.identity?.platform}, not the baseline's machine`);
  let cells = 0;
  let failed = 0;
  const worst = [];
  for (const [release, scenes] of Object.entries(report.releases ?? {})) {
    for (const [scene, result] of Object.entries(scenes)) {
      const base = baseline.releases?.[release]?.[scene];
      const modes = Object.keys(result).filter((key) => result[key] && typeof result[key] === "object" && acceptance.timeField in result[key]);
      for (const mode of modes) {
        cells += 1;
        const cell = `${release} ${scene} ${mode}`;
        if (!sameMachine) {
          failed += 1;
          continue;
        }
        if (!base?.[mode]) {
          failed += 1;
          problems.push(`${cell}: no baseline cell`);
          continue;
        }
        const accepted = acceptance.acceptedOutcomes?.[`${release} ${scene}`];
        if (result.outcome !== base.outcome && accepted !== result.outcome) {
          failed += 1;
          problems.push(`${cell}: outcome ${result.outcome}, the baseline's ${base.outcome}`);
          continue;
        }
        const ratio = result[mode][acceptance.timeField] / base[mode][acceptance.timeField];
        worst.push([ratio, cell]);
        if (!(ratio <= acceptance.maxRatio)) {
          failed += 1;
          problems.push(`${cell}: ${acceptance.timeField} ${ratio.toFixed(2)} times the baseline's, over ${acceptance.maxRatio}`);
        }
      }
    }
  }
  worst.sort((left, right) => right[0] - left[0]);
  return {
    selected: cells, compared: cells, passed: cells - failed, failed, problems: problems.slice(0, 40),
    releases: Object.keys(report.releases ?? {}), worstRatio: worst[0] ? `${worst[0][0].toFixed(2)} (${worst[0][1]})` : null,
  };
}

/** The package: `npm pack --dry-run --json`, every public path of package.json inside it (`exportProblems`). */
function parsePack(text) {
  const start = text.indexOf("[");
  if (start < 0) return undefined;
  let listing;
  try {
    listing = JSON.parse(text.slice(start));
  } catch {
    return undefined;
  }
  const files = (listing[0]?.files ?? []).map((file) => file.path);
  const problems = exportProblems(JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8")), files);
  return { selected: files.length, compared: files.length, passed: files.length - problems.length, failed: problems.length, problems, tarball: listing[0]?.filename, entries: files.length };
}

/** The public paths of a package.json, each of which must be a file of the tarball. */
function publicPaths(pkg) {
  const paths = [];
  const add = (value) => {
    if (typeof value === "string") paths.push(value.replace(/^\.\//, ""));
    else if (value && typeof value === "object") Object.values(value).forEach(add);
  };
  add(pkg.main);
  add(pkg.types);
  add(pkg.typings);
  add(pkg.exports);
  add(pkg.bin);
  return [...new Set(paths)].filter((entry) => !entry.includes("*"));
}

/** What of package.json's public surface the tarball's file list lacks. */
function exportProblems(pkg, files) {
  const listed = new Set(files.map((file) => file.replaceAll("\\", "/")));
  const problems = publicPaths(pkg).filter((entry) => !listed.has(entry)).map((entry) => `${entry} is a public path of package.json and not in the tarball`);
  if (publicPaths(pkg).length === 0) problems.push("package.json names no public path");
  return problems;
}

// ---- jobs ----

const node = process.execPath;
/** The addon napi builds for this host. */
const addonPath = () => path.join(root, "native", "artifacts", `index.${process.platform}-${process.arch}${process.platform === "win32" ? "-msvc" : process.platform === "linux" ? "-gnu" : ""}.node`);
const replay = (flags) => (release, context) => ({
  command: node,
  args: [path.join("replay", "cli.ts"), "--version", release, "--pinned", ...flags],
  env: { REPLAY_RECORDINGS: context.corpus },
  cwd: recorderRoot,
});

/**
 * Every job of the matrix. `scope` is `global` (one unit) or `release` (one unit per release). Each builds its command from the
 * release and the context, and reads its output with `parse`.
 */
const JOBS = [
  { id: "build", scope: "global", title: "the addon and dist/ of HEAD", parse: () => ({ selected: 1, compared: 1, passed: 1, failed: 0 }),
    run: () => ({ command: "npx", args: ["napi", "build", "--manifest-path", "native/Cargo.toml", "--release", "--platform", "--output-dir", "native/artifacts", "--js", "native.js", "--dts", "native.d.ts"], then: [{ command: "npx", args: ["tsc"] }], shell: true }) },
  { id: "cargo-lib", scope: "global", title: "Rust unit tests", parse: parseCargo, run: () => ({ command: "cargo", args: ["test", "--manifest-path", "native/Cargo.toml", "--lib"] }) },
  { id: "cargo-allocations", scope: "global", title: "Rust allocation test", parse: parseCargo, run: () => ({ command: "cargo", args: ["test", "--manifest-path", "native/Cargo.toml", "--release", "--test", "allocations"] }) },
  { id: "js-tests", scope: "global", title: "JavaScript tests", parse: parseNodeTest, run: () => ({ command: node, args: ["--test", "test/*.test.cjs"] }) },
  { id: "recorder-tests", scope: "global", title: "recorder tests", parse: parseNodeTest,
    run: (release, context) => ({ command: node, args: ["--test", "scenarios/test/*.test.ts", "schema/test/*.test.ts", "replay/test/*.test.ts", "runner/test/*.test.ts", "test/*.test.cjs"], env: { REPLAY_RECORDINGS: context.corpus }, cwd: recorderRoot }) },
  { id: "package", scope: "global", title: "npm pack contents and the exports check", parse: parsePack, run: () => ({ command: "npm", args: ["pack", "--dry-run", "--json", "--ignore-scripts"], shell: true }) },
  { id: "bench", scope: "global", title: "benchmarks against the B286 acceptance (audit/bench-acceptance.json)", parse: parseBench, json: true,
    run: (release, context) => ({ command: node, args: [path.join("scripts", "bench", "run.cjs"), "--suite", "ticks", "--releases", context.releases.join(","), "--out", context.json] }) },
  { id: "conformance", scope: "release", title: "the strict gate over the pinned corpus: stepwise, phases, continuous", parse: parseConformance, json: true,
    run: (release, context) => ({ command: node, args: [path.join("replay", "conformance-cli.ts"), "check", "--version", release, "--phases", "--continuous", "--json", context.json], env: { REPLAY_RECORDINGS: context.corpus }, cwd: recorderRoot }) },
  { id: "stepwise-phases", scope: "release", title: "stepwise replay with phases", parse: parseReplay, run: replay(["--phases"]) },
  { id: "continuous", scope: "release", title: "continuous replay", parse: parseReplay, run: replay(["--continuous"]) },
  { id: "projectiles", scope: "release", title: "projectile replay", parse: parseProjectiles, countsDecide: true, run: replay(["--projectiles"]) },
  { id: "moving-server", scope: "release", title: "moving entities, the server's view", parse: parseMoving, countsDecide: true, run: replay(["--moving", "--moving-only"]) },
  { id: "moving-client", scope: "release", title: "moving entities, the client's view", parse: parseMoving, countsDecide: true, run: replay(["--moving-client", "--moving-only"]) },
  { id: "adapter", scope: "release", title: "mineflayer adapter", parse: parseAdapter, run: replay(["--adapter"]) },
  { id: "adapter-motion", scope: "release", title: "mineflayer adapter with its motion", parse: parseAdapter, run: replay(["--adapter", "--motion"]) },
  { id: "bot", scope: "release", title: "a whole mineflayer bot on the recorded packets", parse: parseBot, run: replay(["--bot"]) },
  { id: "bot-carts", scope: "release", title: "the bot's ridden minecarts", parse: parseBotCarts, run: replay(["--bot-carts"]) },
  { id: "determinism", scope: "release", title: "long run: the determinism corpus replayed twice, 5000 ticks a scene", parse: parseDeterminism,
    run: (release, context) => {
      const corpus = path.join(context.work, "determinism-corpus");
      const [first, second] = ["a", "b"].map((side) => path.join(context.work, `determinism-${release}-${side}.json`));
      const addon = addonPath();
      const replayOnce = (out) => ({ command: node, args: [path.join("scripts", "determinism", "replay.cjs"), corpus, addon, out, "--ticks", "5000", "--only", release] });
      return {
        before: fs.existsSync(path.join(corpus, "manifest.json")) ? [] : [{ command: node, args: [path.join("scripts", "determinism", "corpus.cjs"), corpus] }],
        ...replayOnce(first),
        then: [replayOnce(second), { command: node, args: [path.join("scripts", "determinism", "compare.cjs"), first, second] }],
      };
    } },
  { id: "fuzz", scope: "release", title: "randomized vanilla comparison (records through the runner)", parse: parseFuzz,
    // (the campaign compiles its cases into the release's scenario index for its run: the build after it takes them out again,
    // so a later unit's strict gate sees only the pinned scenarios; it runs as the last job for the same reason)
    run: (release, context) => ({
      command: node, args: [path.join("replay", "random-run.ts"), "--version", release, "--seed", "7", "--count", "16", "--work", path.join(context.work, `fuzz-${release}`)], cwd: recorderRoot,
      then: [{ command: node, args: [path.join("scenarios", "build.ts"), "--versions", release], cwd: recorderRoot }],
    }) },
];
const JOB_IDS = JOBS.map((job) => job.id);

// ---- the verdict ----

/** Why a unit's result is no pass; empty when it is one. */
function unitProblems(unit) {
  const problems = [];
  if (unit.status === "blocked") problems.push(`blocked: ${unit.reason ?? "no reason given"}`);
  if (unit.status === "skipped") problems.push("skipped");
  // (a mode whose exit counts what is no failure here, a recording with nothing of its kind, is judged by its counts alone;
  // a crash still leaves no summary to read)
  if (unit.exitCode !== 0 && !unit.countsDecide) problems.push(`exit ${unit.exitCode}`);
  const counts = unit.counts;
  if (!counts) problems.push("no summary was read from its output: missing data");
  else {
    if (!(counts.selected > 0)) problems.push("selected nothing");
    if (!(counts.compared > 0)) problems.push("compared nothing");
    if (counts.failed > 0) problems.push(`${counts.failed} failed`);
    if (counts.known > 0) problems.push(`${counts.known} passed only as known failures`);
  }
  return problems;
}

/** The units a whole matrix needs: each global job once, each release job on every requested release. */
function requiredUnits(releases = requestedReleases(), jobs = JOB_IDS) {
  return JOBS.filter((job) => jobs.includes(job.id)).flatMap((job) => (job.scope === "global" ? [`${job.id}`] : releases.map((release) => `${job.id}@${release}`)));
}

/**
 * Green only when every required unit ran on the record's commit and passed, the tree was clean and known-failures.json is
 * empty. Anything else is red, with each reason. `subset` is the same test over the units the run was asked for, which says
 * whether a partial run was clean and never makes the matrix green.
 */
function evaluate(record, { releases = requestedReleases(), jobs = JOB_IDS } = {}) {
  const reasons = [];
  if (record.dirty) reasons.push(`the tree was dirty: ${record.dirty}`);
  if (record.knownFailures == null) reasons.push("no recorder package was found to read its known-failures.json");
  if (record.knownFailures > 0) reasons.push(`the recorder's known-failures.json has ${record.knownFailures} entr${record.knownFailures === 1 ? "y" : "ies"}`);
  if (!record.corpus?.recordings) reasons.push("no pinned corpus was found");
  const units = new Map((record.units ?? []).map((unit) => [unit.id, unit]));
  const judge = (ids) => {
    const found = [];
    for (const id of ids) {
      const unit = units.get(id);
      if (!unit) {
        found.push(`${id}: not run`);
        continue;
      }
      if (unit.commit !== record.commit) found.push(`${id}: ran on ${unit.commit}, not ${record.commit}`);
      for (const problem of unitProblems(unit)) found.push(`${id}: ${problem}`);
    }
    return found;
  };
  const full = [...reasons, ...judge(requiredUnits(releases, jobs))];
  const asked = record.scope ? requiredUnits(record.scope.releases, record.scope.jobs) : [];
  const subset = asked.length === 0 ? ["nothing was asked for"] : [...reasons, ...judge(asked)];
  return { verdict: full.length === 0 ? "green" : "red", reasons: full, subset: { units: asked.length, clean: subset.length === 0, reasons: subset } };
}

// ---- running ----

const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
const sha256 = (file) => (fs.existsSync(file) ? crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex") : null);

/** The tracked changes of the tree, but the addon, which every build rewrites. */
function dirtiness() {
  const lines = git("status", "--porcelain", "--untracked-files=no").split("\n").filter(Boolean)
    .filter((line) => !/native\/artifacts\/(index\..*\.node|native\.js|native\.d\.ts)$/.test(line));
  return lines.length === 0 ? false : lines.slice(0, 5).map((line) => line.trim()).join("; ") + (lines.length > 5 ? ` (+${lines.length - 5})` : "");
}

/** The recorder the replay jobs ran in: its folder, and the commit of the checkout holding it when it has one. */
function recorderIdentity() {
  if (!fs.existsSync(path.join(recorderRoot, "package.json"))) return { root: recorderRoot.replaceAll("\\", "/"), commit: null };
  let commit = null;
  try {
    commit = execFileSync("git", ["rev-parse", "HEAD"], { cwd: recorderRoot, encoding: "utf8" }).trim();
  } catch {
    // (a recorder outside any checkout has no commit)
  }
  return { root: recorderRoot.replaceAll("\\", "/"), commit };
}

/** The entries of the recorder's known-failures.json; null when there is no recorder to read it from. */
function knownFailureCount() {
  const file = path.join(recorderRoot, "known-failures.json");
  if (!fs.existsSync(file)) return null;
  return Object.values(JSON.parse(fs.readFileSync(file, "utf8"))).reduce((total, layer) => total + Object.keys(layer ?? {}).length, 0);
}

/** The pinned corpus: each manifest's hash and how many of its recordings are on disk. */
function corpusIdentity(corpus, releases) {
  const hash = crypto.createHash("sha256");
  let pinned = 0;
  let present = 0;
  const missing = [];
  for (const release of releases) {
    const manifest = path.join(recorderRoot, "corpus", `${release}.json`);
    if (!fs.existsSync(manifest)) {
      missing.push(`${release}: no manifest`);
      continue;
    }
    const text = fs.readFileSync(manifest);
    hash.update(release).update("\0").update(text);
    for (const entry of JSON.parse(text).entries) {
      pinned += 1;
      if (fs.existsSync(path.join(corpus, release, `${entry.id}.jsonl.gz`))) present += 1;
      else if (missing.length < 20) missing.push(`${release}/${entry.id}`);
    }
  }
  return { root: corpus.replaceAll("\\", "/"), manifests: hash.digest("hex"), recordings: present, pinned, missing };
}

function runOne(step, context, log) {
  return new Promise((resolve) => {
    fs.appendFileSync(log, `\n$ ${step.command} ${step.args.join(" ")}\n`);
    const child = spawn(step.command, step.args, { cwd: step.cwd ?? root, env: { ...process.env, ...context.env, ...step.env }, shell: step.shell ?? false, windowsHide: true });
    let text = "";
    const take = (data) => {
      text += data;
      fs.appendFileSync(log, data);
    };
    child.stdout.on("data", take);
    child.stderr.on("data", take);
    child.on("error", (error) => {
      take(`\n${error.message}\n`);
      resolve({ exitCode: 127, text });
    });
    child.on("close", (code) => resolve({ exitCode: code ?? 1, text }));
  });
}

/** Runs a unit's steps in order (before, the command, then), stopping at the first that fails. */
async function runUnit(job, release, context, record) {
  const id = job.scope === "global" ? job.id : `${job.id}@${release}`;
  const log = path.join(context.work, "logs", `${id.replace(/[^\w.@-]/g, "_")}.log`);
  fs.mkdirSync(path.dirname(log), { recursive: true });
  fs.writeFileSync(log, "");
  const json = job.json ? path.join(context.work, "json", `${id.replace(/[^\w.@-]/g, "_")}.json`) : undefined;
  if (json) {
    fs.mkdirSync(path.dirname(json), { recursive: true });
    fs.rmSync(json, { force: true });
  }
  const plan = job.run(release, { ...context, json });
  const steps = [...(plan.before ?? []), { command: plan.command, args: plan.args, env: plan.env, shell: plan.shell, cwd: plan.cwd }, ...(plan.then ?? [])];
  const startedAt = new Date().toISOString();
  let exitCode = 0;
  let text = "";
  for (const step of steps) {
    const result = await runOne({ ...step, env: { ...plan.env, ...step.env }, shell: step.shell ?? plan.shell }, context, log);
    text += result.text;
    exitCode = result.exitCode;
    if (exitCode !== 0) break;
  }
  const counts = job.parse(text, { json });
  const unit = {
    id, job: job.id, release: job.scope === "global" ? null : release, commit: record.commit,
    command: steps.map((step) => [step.command, ...step.args].join(" ")).join(" && "),
    startedAt, endedAt: new Date().toISOString(), exitCode, ...(job.countsDecide ? { countsDecide: true } : {}), counts: counts ?? null,
    log: log.replaceAll("\\", "/"), tail: text.trim().split("\n").slice(-3).join("\n").slice(-600),
  };
  const problems = unitProblems(unit);
  unit.status = problems.length === 0 ? "pass" : "fail";
  if (problems.length > 0) unit.problems = problems;
  return unit;
}

function writeRecord(file, record) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const evaluation = evaluate(record);
  record.verdict = evaluation.verdict;
  record.reasons = evaluation.reasons.slice(0, 200);
  record.reasonCount = evaluation.reasons.length;
  record.subset = evaluation.subset;
  fs.writeFileSync(file, `${JSON.stringify(record, null, 1)}\n`);
}

const option = (args, name) => {
  const at = args.indexOf(`--${name}`);
  return at >= 0 && at + 1 < args.length && !args[at + 1].startsWith("--") ? args[at + 1] : undefined;
};

function latestRecord(commit) {
  if (!fs.existsSync(recordDirectory)) return undefined;
  const files = fs.readdirSync(recordDirectory).filter((name) => name.endsWith(".json") && (!commit || name.startsWith(commit.slice(0, 12)))).sort();
  return files.length === 0 ? undefined : path.join(recordDirectory, files.at(-1));
}

async function main(args) {
  if (args.includes("--list")) {
    for (const job of JOBS) {
      const plan = job.run("<release>", { corpus: "<corpus>", work: "<work>", releases: ["<release>"], json: "<json>" });
      console.log(`${job.id.padEnd(18)} ${job.scope.padEnd(8)} ${job.title}\n${"".padEnd(28)}${[plan.command, ...plan.args].join(" ")}`);
    }
    return 0;
  }
  if (args.includes("--doc")) {
    const file = option(args, "doc") ?? latestRecord();
    if (!file) throw new Error("no record in audit/release-matrix");
    fs.writeFileSync(docPath, matrixDoc(JSON.parse(fs.readFileSync(file, "utf8")), file));
    console.log(`wrote ${path.relative(root, docPath)} from ${path.relative(root, file)}`);
    return 0;
  }
  const commit = git("rev-parse", "HEAD");
  const all = requestedReleases();
  const releases = (option(args, "releases") ?? all.join(",")).split(",");
  const unknownRelease = releases.filter((release) => !all.includes(release));
  if (unknownRelease.length > 0) throw new Error(`not registered: ${unknownRelease.join(", ")}`);
  const jobs = (option(args, "jobs") ?? JOB_IDS.join(",")).split(",");
  const unknownJob = jobs.filter((job) => !JOB_IDS.includes(job));
  if (unknownJob.length > 0) throw new Error(`no such job: ${unknownJob.join(", ")} (jobs: ${JOB_IDS.join(", ")})`);
  const corpus = path.resolve(option(args, "corpus") ?? process.env.REPLAY_RECORDINGS ?? "E:/mc-physics-demo-wt/corpus");
  const work = path.resolve(option(args, "work") ?? path.join(os.tmpdir(), "release-matrix", commit.slice(0, 12)));
  fs.mkdirSync(work, { recursive: true });

  let file;
  let record;
  if (args.includes("--resume")) {
    file = option(args, "resume") ?? latestRecord(commit);
    if (!file || !fs.existsSync(file)) throw new Error(`--resume: no record of ${commit.slice(0, 12)} in audit/release-matrix`);
    record = JSON.parse(fs.readFileSync(file, "utf8"));
    if (record.commit !== commit) throw new Error(`--resume: ${path.relative(root, file)} is of ${record.commit}, HEAD is ${commit}`);
    record.scope = { jobs: [...new Set([...record.scope.jobs, ...jobs])], releases: [...new Set([...record.scope.releases, ...releases])] };
  } else {
    const date = new Date().toISOString().slice(0, 10);
    file = path.join(recordDirectory, `${commit.slice(0, 12)}-${date}.json`);
    record = { schema: 1, generatedBy: "scripts/release-matrix.cjs", commit, units: [], scope: { jobs, releases } };
  }
  const { engineDigest } = require("./parity-contract.cjs");
  const addon = addonPath();
  Object.assign(record, {
    engine: engineDigest(),
    dirty: dirtiness(),
    knownFailures: knownFailureCount(),
    recorder: recorderIdentity(),
    platform: `${process.platform}-${process.arch}`,
    node: process.version,
    // (of every release the record has asked for, a resumed one's too)
    corpus: corpusIdentity(corpus, record.scope.releases),
  });
  record.startedAt ??= new Date().toISOString();
  const context = { corpus, work, releases, env: { CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "4" } };
  const done = new Set(args.includes("--rerun") ? [] : record.units.filter((unit) => unit.status === "pass").map((unit) => unit.id));
  for (const job of JOBS.filter((entry) => jobs.includes(entry.id))) {
    for (const release of job.scope === "global" ? [null] : releases) {
      const id = job.scope === "global" ? job.id : `${job.id}@${release}`;
      if (done.has(id)) continue;
      process.stdout.write(`${id} ... `);
      const unit = await runUnit(job, release, context, record);
      record.units = [...record.units.filter((entry) => entry.id !== id), unit];
      if (job.id === "build") record.addon = { file: path.relative(root, addon).replaceAll("\\", "/"), sha256: sha256(addon) };
      record.endedAt = new Date().toISOString();
      writeRecord(file, record);
      console.log(`${unit.status}${unit.problems ? ` (${unit.problems.join("; ")})` : ""} ${JSON.stringify(unit.counts ?? {})}`);
    }
  }
  record.addon ??= { file: path.relative(root, addon).replaceAll("\\", "/"), sha256: sha256(addon) };
  record.endedAt = new Date().toISOString();
  writeRecord(file, record);
  fs.writeFileSync(docPath, matrixDoc(record, file));
  console.log(`${path.relative(root, file)}: ${record.verdict}, ${record.reasonCount} reason(s); asked-for units ${record.subset.clean ? "clean" : "not clean"} (${record.subset.units})`);
  return record.verdict === "green" ? 0 : 1;
}

// ---- the page ----

function matrixDoc(record, file) {
  const units = record.units ?? [];
  const rows = JOBS.map((job) => {
    const mine = units.filter((unit) => unit.job === job.id);
    const passed = mine.filter((unit) => unit.status === "pass").length;
    const need = job.scope === "global" ? 1 : requestedReleases().length;
    const sum = (key) => mine.reduce((total, unit) => total + (unit.counts?.[key] ?? 0), 0);
    return `| ${job.id} | ${job.scope} | ${passed} of ${need} | ${mine.length - passed} | ${sum("compared")} | ${sum("failed")} | ${job.title} |`;
  });
  const reasons = (record.reasons ?? []).slice(0, 15).map((reason) => `- ${reason}`);
  return `# Release matrix

Generated by \`node scripts/release-matrix.cjs\` from \`${path.relative(root, file).replaceAll("\\", "/")}\`: do not edit. The record keeps every
unit's command, commit, times and counts.

Commit \`${record.commit}\`, engine \`${(record.engine ?? "").slice(0, 12)}\`, ${record.platform}, Node ${record.node}, from ${record.startedAt} to ${record.endedAt}.
Corpus: ${record.corpus?.recordings ?? 0} of ${record.corpus?.pinned ?? 0} pinned recordings found under \`${record.corpus?.root}\`. Known failures: ${record.knownFailures}.
Asked for: ${record.scope?.jobs?.length} job(s) on ${record.scope?.releases?.length} release(s).

**Verdict: ${record.verdict}.** ${record.verdict === "green" ? "Every required unit passed on this commit." : `${record.reasonCount} reason(s); the asked-for units were ${record.subset?.clean ? "clean, which by itself certifies nothing" : "not clean"}.`}

A unit passes only when its command exits 0, its summary was read, it selected and compared at least one case and failed none.
A unit that was not run, ran on nothing, or passed only as a known failure is no pass.

| job | scope | passed | not passing | compared | failed | what |
|---|---|---:|---:|---:|---:|---|
${rows.join("\n")}

${reasons.length === 0 ? "" : `First reasons:\n\n${reasons.join("\n")}\n`}`;
}

module.exports = { JOBS, JOB_IDS, benchVerdict, evaluate, unitProblems, requiredUnits, exportProblems, publicPaths, parsers: { parseCargo, parseNodeTest, parseReplay, parseMoving, parseProjectiles, parseAdapter, parseBot, parseBotCarts, parseConformance, parseFuzz, parseDeterminism, parsePack }, latestRecord, recordDirectory };

if (require.main === module) {
  main(process.argv.slice(2)).then((code) => process.exit(code), (error) => {
    console.error(error.message);
    process.exit(2);
  });
}
