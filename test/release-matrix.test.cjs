// The release gate's tooling (B292, B288, B295): a matrix is never green on nothing, an evidence row never passes on unknown,
// skipped or blocked evidence, and the release notes must name what the tarball holds.
const test = require("node:test");
const assert = require("node:assert/strict");
const matrix = require("../scripts/release-matrix.cjs");
const evidence = require("../scripts/release-evidence.cjs");
const notes = require("../scripts/release-notes.cjs");

const COMMIT = "a".repeat(40);
const counts = (extra = {}) => ({ selected: 10, compared: 10, passed: 10, failed: 0, ...extra });
const unit = (id, extra = {}) => ({ id, job: id.split("@")[0], release: id.split("@")[1] ?? null, commit: COMMIT, exitCode: 0, status: "pass", counts: counts(), ...extra });
const scope = { jobs: ["cargo-lib", "projectiles"], releases: ["1.20.4", "26.3"] };
const passing = () => ({
  commit: COMMIT, dirty: false, knownFailures: 0, corpus: { recordings: 100, pinned: 100 }, scope,
  units: ["cargo-lib", "projectiles@1.20.4", "projectiles@26.3"].map((id) => unit(id)),
});
const judge = (record) => matrix.evaluate(record, scope);

test("a matrix whose every required unit passed on its commit is green", () => {
  assert.equal(judge(passing()).verdict, "green");
});

test("a unit that selected or compared nothing makes the matrix red", () => {
  for (const empty of [{ selected: 0, compared: 0, passed: 0 }, { compared: 0, passed: 0 }]) {
    const record = passing();
    record.units[1].counts = counts(empty);
    const result = judge(record);
    assert.equal(result.verdict, "red");
    assert.match(result.reasons.join("\n"), /projectiles@1\.20\.4: (selected|compared) nothing/);
  }
});

test("a unit whose summary was not read is missing data, and red", () => {
  const record = passing();
  record.units[0].counts = null;
  assert.match(judge(record).reasons.join("\n"), /cargo-lib: no summary was read/);
  assert.equal(matrix.parsers.parseReplay("nothing here"), undefined);
  assert.equal(matrix.parsers.parseCargo("error[E0425]: cannot find value"), undefined);
});

test("a unit not run, of another commit, blocked or skipped is no pass", () => {
  const missing = passing();
  missing.units.pop();
  assert.match(judge(missing).reasons.join("\n"), /projectiles@26\.3: not run/);
  const stale = passing();
  stale.units[2].commit = "b".repeat(40);
  assert.match(judge(stale).reasons.join("\n"), /ran on b+, not a+/);
  for (const status of ["blocked", "skipped"]) {
    const record = passing();
    record.units[2].status = status;
    assert.equal(judge(record).verdict, "red", status);
  }
});

test("known failures, a dirty tree, a missing corpus or a failed case make the matrix red", () => {
  const known = passing();
  known.knownFailures = 1;
  assert.match(judge(known).reasons.join("\n"), /known-failures\.json has 1 entry/);
  const dirty = passing();
  dirty.dirty = "M native/src/lib.rs";
  assert.equal(judge(dirty).verdict, "red");
  const corpus = passing();
  corpus.corpus = { recordings: 0, pinned: 0 };
  assert.equal(judge(corpus).verdict, "red");
  const failed = passing();
  failed.units[1].counts = counts({ failed: 1 });
  assert.equal(judge(failed).verdict, "red");
  const knownCase = passing();
  knownCase.units[1].counts = counts({ known: 2 });
  assert.match(judge(knownCase).reasons.join("\n"), /passed only as known failures/);
  const exit = passing();
  exit.units[0].exitCode = 101;
  assert.match(judge(exit).reasons.join("\n"), /cargo-lib: exit 101/);
});

test("a clean subset run is clean as a subset and still red as the matrix", () => {
  const record = passing();
  const result = matrix.evaluate(record, { releases: ["1.20.4", "26.3", "1.8.8"], jobs: scope.jobs });
  assert.equal(result.verdict, "red");
  assert.equal(result.subset.clean, true);
  assert.match(result.reasons.join("\n"), /projectiles@1\.8\.8: not run/);
});

test("the parsers read each mode's summary into counts", () => {
  assert.deepEqual(matrix.parsers.parseCargo("test result: ok. 5 passed; 0 failed; 1 ignored; 0 measured\ntest result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured"),
    { selected: 9, compared: 8, passed: 7, failed: 1, skipped: 1 });
  assert.deepEqual(matrix.parsers.parseReplay("3 recording(s), stepwise with phases, 2 matching, 0 known, 1 unsupported, 0 untested, 0 unexpected"),
    { selected: 3, compared: 2, passed: 2, known: 0, unsupported: 1, untested: 0, failed: 0 });
  const projectiles = "ok    1.20.4/a (1 projectile(s), 5 ticks)\nnone  1.20.4/b: no projectile tick to compare\nBAD   1.20.4/c: x\n3 recording(s), 1 matching, 2 unexpected";
  assert.deepEqual(matrix.parsers.parseProjectiles(projectiles), { selected: 3, compared: 2, passed: 1, untested: 1, failed: 1 });
  assert.deepEqual(matrix.parsers.parseBot("4 recording(s), bot: 2 matching, 1 unsupported, 1 past the hands, 0 codec, 0 different"),
    { selected: 4, compared: 3, passed: 2, unsupported: 1, failed: 1 });
  assert.deepEqual(matrix.parsers.parseDeterminism("win32 (abc) against win32 (abc): 12 runs, 60000 ticks, 0 runs ended by the same refusal, 0 differences"),
    { selected: 12, compared: 12, passed: 12, failed: 0, ticks: 60000 });
  assert.equal(matrix.parsers.parseFuzz("seed 7: 16 generated, 14 compared, 0 different, 0 invalid, 2 unsupported, 0 nondeterministic, 0 missing").failed, 0);
});

test("every public path of package.json must be in the tarball", () => {
  const pkg = { main: "dist/index.js", types: "dist/index.d.ts" };
  assert.deepEqual(matrix.exportProblems(pkg, ["dist/index.js", "dist/index.d.ts"]), []);
  assert.match(matrix.exportProblems(pkg, ["dist/index.js"]).join(), /dist\/index\.d\.ts is a public path/);
  assert.match(matrix.exportProblems({}, []).join(), /names no public path/);
});

// ---- evidence rows ----

const REQUIREMENT_IDS = evidence.REQUIREMENTS.map((requirement) => requirement.id);
const row = (state = "pass") => ({ release: "1.20.4", cells: Object.fromEntries(REQUIREMENT_IDS.map((id) => [id, { state, why: "" }])) });

test("there are twelve per-version requirements, B276 to B287", () => {
  assert.deepEqual(REQUIREMENT_IDS, Array.from({ length: 12 }, (_, index) => `B${276 + index}`));
});

test("a row with any unknown, skipped, blocked, stale or failed cell is not complete", () => {
  assert.equal(evidence.rowComplete(row()), true);
  for (const state of ["unknown", "skipped", "blocked", "stale", "fail", undefined]) {
    const open = row();
    open.cells.B283 = { state, why: "" };
    assert.equal(evidence.rowComplete(open), false, String(state));
  }
  const short = row();
  delete short.cells.B287;
  assert.equal(evidence.rowComplete(short), false);
});

test("a matrix cell passes only on units that ran and passed on the record's commit", () => {
  const record = passing();
  assert.equal(evidence.unitsCell(record, ["cargo-lib"], "x").state, "pass");
  assert.equal(evidence.unitsCell(undefined, ["cargo-lib"], "x").state, "unknown");
  assert.equal(evidence.unitsCell(record, ["js-tests"], "x").state, "unknown");
  for (const [status, state] of [["blocked", "blocked"], ["skipped", "skipped"]]) {
    const changed = passing();
    changed.units[0].status = status;
    assert.equal(evidence.unitsCell(changed, ["cargo-lib"], "x").state, state);
  }
  const stale = passing();
  stale.units[0].commit = "b".repeat(40);
  assert.equal(evidence.unitsCell(stale, ["cargo-lib"], "x").state, "stale");
  const empty = passing();
  empty.units[0].counts = counts({ compared: 0 });
  assert.equal(evidence.unitsCell(empty, ["cargo-lib"], "x").state, "fail");
  assert.equal(evidence.both({ state: "pass", why: "" }, { state: "unknown", why: "" }).state, "unknown");
  // (a unit that ran and failed is reported as a failure even when another of the cell's units did not run)
  const failedAndMissing = passing();
  failedAndMissing.units[1].counts = counts({ failed: 3 });
  assert.equal(evidence.unitsCell(failedAndMissing, ["projectiles@1.20.4", "bot@1.20.4"], "x").state, "fail");
});

test("a row with no matrix record is complete for nothing", () => {
  const inputs = { admissions: {}, registered: ["1.20.4"], shared: undefined, subsystems: undefined };
  const built = evidence.evidenceRow("1.20.4", undefined, inputs);
  assert.equal(evidence.rowComplete(built), false);
  assert.ok(Object.values(built.cells).every((entry) => entry.state !== "pass" || entry.why.startsWith("registered")));
});

test("the published list must agree exactly with the approved rows", () => {
  const complete = { ...row(), complete: true };
  const open = { ...row("unknown"), release: "26.3", complete: false };
  const inputs = (published, approvedBy = "lead, 2026-11-10") => ({
    requested: { approvedBy, releases: ["1.20.4", "26.3"] }, registered: ["1.20.4", "26.3"], published: { releases: published },
  });
  assert.deepEqual(evidence.publishedProblems([complete, open], inputs({ "1.20.4": { admitted: true }, "26.3": { admitted: false } })), []);
  assert.match(evidence.publishedProblems([complete, open], inputs({ "1.20.4": { admitted: true }, "26.3": { admitted: true } })).join(), /26\.3 is published as admitted/);
  assert.match(evidence.publishedProblems([complete, open], inputs({ "1.20.4": { admitted: false }, "26.3": { admitted: false } })).join(), /1\.20\.4 has a complete evidence row and is not published/);
  assert.match(evidence.publishedProblems([complete, open], inputs({}, null)).join(), /not been approved/);
  const extra = inputs({});
  extra.registered = ["1.20.4", "26.3", "1.8.8"];
  assert.match(evidence.publishedProblems([complete, open], extra).join(), /1\.8\.8 is registered and not requested/);
});

// ---- release notes ----

test("the notes must name the tarball's main entry and public paths", () => {
  const pkg = { main: "dist/index.js", types: "dist/index.d.ts" };
  const files = ["dist/index.js", "dist/index.d.ts", "dist/native.js", "native/artifacts/native.js"];
  const text = "| Main entry | `dist/index.js` |\n| Other public path | `dist/native`, and the WebAssembly loaders under `native/artifacts` |\n";
  assert.deepEqual(notes.notesPackProblems(text, pkg, files), []);
  assert.match(notes.notesPackProblems(text.replace("dist/index.js", "lib/index.js"), pkg, files).join(), /main entry/);
  assert.match(notes.notesPackProblems(text, pkg, files.filter((file) => !file.startsWith("native/"))).join(), /native\/artifacts, a public path of the notes/);
  assert.match(notes.notesPackProblems("", pkg, files).join(), /no main entry/);
});

// ---- the bench acceptance (B286) ----

test("the bench job fails a regression, a changed outcome, a missing baseline cell and another machine", () => {
  const acceptance = { machine: { platform: "win32-x64", cpu: "cpu A" }, timeField: "minCpuUs", maxRatio: 2, acceptedOutcomes: {} };
  const baseline = { releases: { "1.20.4": { empty: { outcome: "o1", scalar: { minCpuUs: 10 } } } } };
  const report = (minCpuUs, outcome = "o1", cpu = "cpu A", scene = "empty") => ({ identity: { platform: "win32-x64", cpu }, releases: { "1.20.4": { [scene]: { outcome, scalar: { minCpuUs } } } } });
  const within = matrix.benchVerdict(report(19.9), acceptance, baseline);
  assert.deepEqual([within.compared, within.failed, within.releases], [1, 0, ["1.20.4"]]);
  assert.match(matrix.benchVerdict(report(20.1), acceptance, baseline).problems.join(), /2\.01 times the baseline's/);
  assert.match(matrix.benchVerdict(report(10, "o2"), acceptance, baseline).problems.join(), /outcome o2/);
  assert.equal(matrix.benchVerdict(report(10, "o2"), { ...acceptance, acceptedOutcomes: { "1.20.4 empty": "o2" } }, baseline).failed, 0);
  assert.match(matrix.benchVerdict(report(10, "o1", "cpu A", "dense"), acceptance, baseline).problems.join(), /no baseline cell/);
  const elsewhere = matrix.benchVerdict(report(10, "o1", "cpu B"), acceptance, baseline);
  assert.equal(elsewhere.failed, 1);
  assert.match(elsewhere.problems.join(), /not the baseline's machine/);
});

test("B286 passes only where the accepted bench measured the release", () => {
  const record = passing();
  record.units.push(unit("bench", { counts: counts({ releases: ["1.20.4"] }) }), unit("cargo-allocations"));
  const inputs = { admissions: {}, registered: ["1.20.4", "26.3"] };
  assert.equal(evidence.evidenceRow("1.20.4", record, inputs).cells.B286.state, "pass");
  assert.equal(evidence.evidenceRow("26.3", record, inputs).cells.B286.state, "unknown");
});
