// Per-release completion evidence (B288): a dated row for every requested release, one cell per per-version requirement
// (B276-B287), each from the release matrix record (scripts/release-matrix.cjs) and the generated evidence on record.
//
//   node scripts/release-evidence.cjs [record.json]           writes audit/release-evidence/<commit>-<date>.json and
//                                                             docs/release-evidence.md from that record (default: HEAD's latest)
//   node scripts/release-evidence.cjs --check [record.json]   also fails unless the published list agrees with the rows
//
// A cell is `pass`, `fail`, `stale` (evidence of another commit or engine), `unknown` (no evidence), `skipped` or `blocked`.
// Only `pass` passes: a row is complete when all twelve cells pass, and nothing else counts (`rowComplete`).
//
// The approved release set is audit/release-requested.json. The published list is what the package says: SUPPORTED_VERSIONS
// and src/release-status.json's `admitted`. They must agree with the rows exactly (`publishedProblems`): every requested
// release registered and no other, and a release published as admitted exactly when its row is complete.
"use strict";
const fs = require("node:fs");
const path = require("node:path");
const matrix = require("./release-matrix.cjs");

const root = path.resolve(__dirname, "..");
const requestedPath = path.join(root, "audit", "release-requested.json");
const evidenceDirectory = path.join(root, "audit", "release-evidence");
const docPath = path.join(root, "docs", "release-evidence.md");

const PASSING = "pass";

/** The twelve per-version requirements, each with the ticket that defines it. */
const REQUIREMENTS = [
  { id: "B276", title: "exact source and mappings" },
  { id: "B277", title: "complete generated data" },
  { id: "B278", title: "its own engine module" },
  { id: "B279", title: "one consistent construction path" },
  { id: "B280", title: "no incompatible shared fallback" },
  { id: "B281", title: "every physics subsystem evidenced" },
  { id: "B282", title: "vanilla differential with zero unexplained mismatches" },
  { id: "B283", title: "long-run deterministic replay" },
  { id: "B284", title: "N-API and TypeScript integration" },
  { id: "B285", title: "package loads from a clean install" },
  { id: "B286", title: "performance and memory measured and accepted" },
  { id: "B287", title: "provenance and limitations cite current evidence" },
];

const cell = (state, why) => ({ state, why });

/** A row is complete only when every requirement passes: unknown, skipped, blocked, stale and fail all keep it open. */
const rowComplete = (row) => REQUIREMENTS.every((requirement) => row.cells[requirement.id]?.state === PASSING);

/** The cell of a set of matrix units: pass only when every one ran on the record's commit and passed. */
function unitsCell(record, ids, what) {
  const units = new Map((record?.units ?? []).map((unit) => [unit.id, unit]));
  const missing = ids.filter((id) => !units.has(id));
  if (!record) return cell("unknown", "no matrix record");
  if (record.dirty) return cell("fail", `the matrix ran on a dirty tree: ${record.dirty}`);
  const ran = ids.filter((id) => units.has(id));
  // (a unit that ran and failed is the worst news, whatever else did not run)
  const failing = ran.filter((id) => !["blocked", "skipped"].includes(units.get(id).status) && units.get(id).commit === record.commit && matrix.unitProblems(units.get(id)).length > 0);
  if (failing.length > 0) return cell("fail", `${what}: ${failing.map((id) => `${id} (${matrix.unitProblems(units.get(id))[0]})`).join(", ")}${missing.length > 0 ? `; ${missing.length} not run` : ""}`);
  const blocked = ran.filter((id) => units.get(id).status === "blocked");
  if (blocked.length > 0) return cell("blocked", `${what}: ${blocked.join(", ")} blocked`);
  const skipped = ran.filter((id) => units.get(id).status === "skipped");
  if (skipped.length > 0) return cell("skipped", `${what}: ${skipped.join(", ")} skipped`);
  const stale = ran.filter((id) => units.get(id).commit !== record.commit);
  if (stale.length > 0) return cell("stale", `${what}: ${stale.join(", ")} ran on another commit`);
  if (missing.length > 0) return cell("unknown", `${what}: ${missing.join(", ")} not run`);
  return cell(PASSING, `${what}: ${ids.join(", ")} passed on ${record.commit.slice(0, 12)}`);
}

/** Both cells must pass for the combination to; otherwise the first that does not, in order of severity. */
function both(...cells) {
  for (const state of ["fail", "blocked", "skipped", "stale", "unknown"]) {
    const found = cells.find((entry) => entry.state === state);
    if (found) return found;
  }
  return cell(PASSING, cells.map((entry) => entry.why).join("; "));
}

const readJson = (file) => (fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, "utf8")) : undefined);

/** The inputs the rows read besides the matrix record. Tests hand their own. */
function gatherInputs() {
  const { allAdmissions } = require("./release-admission.cjs");
  return {
    admissions: allAdmissions(),
    registered: require("./registered-releases.cjs").registeredReleases().map((entry) => entry.version),
    shared: readJson(path.join(root, "generated", "shared-equivalence.json")),
    subsystems: readJson(path.join(root, "generated", "subsystem-matrix.json")),
    requested: readJson(requestedPath),
    published: readJson(path.join(root, "src", "release-status.json")),
  };
}

/** One release's row: each requirement's cell, from the record and the inputs. */
function evidenceRow(release, record, inputs) {
  const admission = inputs.admissions?.[release];
  const cells = {};
  // B276, B277: the admission's source and data dimensions, which recompute from pinned identities
  cells.B276 = !admission ? cell("unknown", "no admission")
    : ["exact", "alias"].includes(admission.source.status) ? cell(PASSING, `source ${admission.source.status}`)
      : cell(admission.source.status === "blocked" ? "blocked" : "fail", `source ${admission.source.status}: ${admission.source.reasons.join("; ")}`);
  cells.B277 = !admission ? cell("unknown", "no admission")
    : admission.data.status === "complete" ? cell(PASSING, "data complete") : cell("fail", `data ${admission.data.status}: ${admission.data.reasons.join("; ")}`);
  // B278: registered with its own module, and the Rust and registry tests of this commit pass
  cells.B278 = both(
    inputs.registered?.includes(release) ? cell(PASSING, "registered in SUPPORTED_VERSIONS") : cell("fail", "not registered"),
    unitsCell(record, ["cargo-lib", "js-tests"], "module and registry tests"),
  );
  // B279: the construction tests are in the JavaScript suite
  cells.B279 = unitsCell(record, ["js-tests"], "construction tests");
  // B280: every shared path explained, every profile rule checked
  const totals = inputs.shared?.totals;
  cells.B280 = !totals ? cell("unknown", "no generated/shared-equivalence.json")
    : totals.unresolved === 0 && totals.rulesDiffer === 0 && totals.rulesUnverified === 0 && (inputs.shared.staleReviews ?? []).length === 0
      ? cell(PASSING, `shared paths: ${totals.rulesChecked} rules checked, none unresolved`)
      : cell("fail", `shared paths: ${totals.unresolved} unresolved, ${totals.rulesDiffer} rules differ, ${totals.rulesUnverified} unverified`);
  // B281: each family's evidence for the release, from runs of the record's commit
  cells.B281 = subsystemCell(release, record, inputs.subsystems);
  // B282: every replay mode over the pinned corpus
  cells.B282 = unitsCell(record, ["conformance", "stepwise-phases", "projectiles", "moving-server", "moving-client", "adapter", "adapter-motion", "bot", "bot-carts", "fuzz"].map((job) => `${job}@${release}`), "differential replays");
  // B283: continuous replay and the long determinism run
  cells.B283 = unitsCell(record, [`continuous@${release}`, `determinism@${release}`], "continuous and long runs");
  // B284: the integration suites on the built addon
  cells.B284 = unitsCell(record, ["build", "js-tests", "recorder-tests"], "integration");
  // B285: the package and its exports
  cells.B285 = unitsCell(record, ["package"], "package");
  // B286: the bench job held to audit/bench-acceptance.json covered the release, and the allocation test passed
  const bench = unitsCell(record, ["bench", "cargo-allocations"], "benchmarks and allocations");
  const covered = (record?.units ?? []).find((unit) => unit.id === "bench")?.counts?.releases ?? [];
  cells.B286 = bench.state !== PASSING ? bench
    : covered.includes(release) ? cell(PASSING, `${bench.why}; within the acceptance`) : cell("unknown", `${bench.why}; the bench did not measure ${release}`);
  // B287: completion claims cite current pins: the admission's conformance dimension
  cells.B287 = !admission ? cell("unknown", "no admission")
    : admission.conformance?.status === "verified" ? cell(PASSING, "conformance pins current")
      : cell("fail", `conformance ${admission.conformance?.status ?? "unknown"}: ${(admission.conformance?.reasons ?? []).slice(0, 2).join("; ")}`);
  return { release, date: new Date().toISOString().slice(0, 10), commit: record?.commit ?? null, cells, complete: false };
}

function subsystemCell(release, record, subsystems) {
  if (!subsystems) return cell("unknown", "no generated/subsystem-matrix.json");
  const commits = Object.values(subsystems.runs ?? {}).flat().map((run) => run.commit);
  if (!record || !commits.includes(record.commit)) return cell("stale", `subsystem runs are of ${[...new Set(commits)].map((commit) => commit.slice(0, 8)).join(", ") || "nothing"}, not the matrix commit`);
  const bad = [];
  for (const family of subsystems.families ?? []) {
    const evidence = subsystems.evidence?.[family.id]?.[release];
    if (!evidence) {
      bad.push(`${family.id}: no evidence`);
      continue;
    }
    for (const [mode, result] of Object.entries(evidence)) {
      if (result === "n/a") continue;
      if (result.failed > 0 || (result.missing ?? []).length > 0 || !(result.compared > 0)) bad.push(`${family.id} ${mode}`);
    }
  }
  return bad.length === 0 ? cell(PASSING, "every family evidenced") : cell("fail", `families: ${bad.slice(0, 4).join(", ")}${bad.length > 4 ? ` (+${bad.length - 4})` : ""}`);
}

/** Every row, complete or not. */
function evidenceRows(record, inputs, releases) {
  return releases.map((release) => {
    const row = evidenceRow(release, record, inputs);
    row.complete = rowComplete(row);
    return row;
  });
}

/** Where the published list and the approved set disagree with the rows. Empty when they agree exactly. */
function publishedProblems(rows, inputs) {
  const problems = [];
  const requested = inputs.requested?.releases ?? [];
  if (!inputs.requested) problems.push("no approved release set: audit/release-requested.json is missing");
  else if (!inputs.requested.approvedBy) problems.push("the requested release set has not been approved (audit/release-requested.json approvedBy is empty)");
  const registered = inputs.registered ?? [];
  for (const release of requested) if (!registered.includes(release)) problems.push(`${release} is requested and not registered`);
  for (const release of registered) if (!requested.includes(release)) problems.push(`${release} is registered and not requested`);
  const published = inputs.published?.releases ?? {};
  for (const row of rows) {
    const admitted = published[row.release]?.admitted === true;
    if (admitted && !row.complete) problems.push(`${row.release} is published as admitted and its evidence row is not complete`);
    if (!admitted && row.complete) problems.push(`${row.release} has a complete evidence row and is not published as admitted`);
  }
  for (const release of Object.keys(published)) if (!rows.some((row) => row.release === release)) problems.push(`${release} is published and has no evidence row`);
  return problems;
}

function evidenceDoc(file, record, rows, problems) {
  const header = `| release | ${REQUIREMENTS.map((requirement) => requirement.id).join(" | ")} | complete |`;
  const rule = `|---|${REQUIREMENTS.map(() => "---").join("|")}|---|`;
  const body = rows.map((row) => `| ${row.release} | ${REQUIREMENTS.map((requirement) => row.cells[requirement.id].state).join(" | ")} | ${row.complete ? "yes" : "no"} |`);
  return `# Release evidence

Generated by \`node scripts/release-evidence.cjs\` into \`${path.relative(root, file).replaceAll("\\", "/")}\`: do not edit. One dated row per requested
release, one cell per per-version requirement. A cell is \`pass\` only on current evidence; \`unknown\`, \`skipped\`, \`blocked\`,
\`stale\` and \`fail\` all leave the row incomplete. Each cell's reason is in the record.

Matrix record: \`${record ? record.commit : "none"}\` (${record?.verdict ?? "no verdict"}). ${rows.filter((row) => row.complete).length} of ${rows.length} rows complete.

${REQUIREMENTS.map((requirement) => `- ${requirement.id}: ${requirement.title}`).join("\n")}

${header}
${rule}
${body.join("\n")}

${problems.length === 0 ? "The published list agrees with the rows." : `The published list does not agree with the rows:\n\n${problems.slice(0, 20).map((problem) => `- ${problem}`).join("\n")}${problems.length > 20 ? `\n- (+${problems.length - 20})` : ""}`}
`;
}

function main(args) {
  const recordFile = args.find((arg) => !arg.startsWith("--")) ?? matrix.latestRecord();
  const record = recordFile ? JSON.parse(fs.readFileSync(recordFile, "utf8")) : undefined;
  const inputs = gatherInputs();
  const releases = inputs.requested?.releases ?? inputs.registered;
  const rows = evidenceRows(record, inputs, releases);
  const problems = publishedProblems(rows, inputs);
  const date = new Date().toISOString().slice(0, 10);
  const file = path.join(evidenceDirectory, `${(record?.commit ?? "none").slice(0, 12)}-${date}.json`);
  fs.mkdirSync(evidenceDirectory, { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify({ schema: 1, generatedBy: "scripts/release-evidence.cjs", date, record: recordFile ? path.relative(root, recordFile).replaceAll("\\", "/") : null, commit: record?.commit ?? null, requirements: REQUIREMENTS, rows, publishedProblems: problems }, null, 1)}\n`);
  fs.writeFileSync(docPath, evidenceDoc(file, record, rows, problems));
  console.log(`${path.relative(root, file)}: ${rows.filter((row) => row.complete).length} of ${rows.length} rows complete; ${problems.length} published-list problem(s)`);
  if (args.includes("--check") && (problems.length > 0 || rows.some((row) => !row.complete))) return 1;
  return 0;
}

module.exports = { REQUIREMENTS, evidenceRow, evidenceRows, rowComplete, publishedProblems, unitsCell, both };

if (require.main === module) process.exitCode = main(process.argv.slice(2));
