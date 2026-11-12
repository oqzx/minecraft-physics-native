// The generated part of docs/release-notes.md (B295): the final pass result from the release matrix record and the evidence
// rows, the supported releases, and the package as `npm pack` lists it.
//
//   node scripts/release-notes.cjs            rewrites the block between the markers from HEAD's latest records
//   node scripts/release-notes.cjs --check    fails when the block is not what the records give, or when the notes and the
//                                             tarball disagree (`notesPackProblems`)
//
// The block says what a record shows and nothing more: with no record, or a red one, it says so, and no release is listed as
// supported unless its evidence row is complete.
"use strict";
const { execFileSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");
const matrix = require("./release-matrix.cjs");

const root = path.resolve(__dirname, "..");
const notesPath = path.join(root, "docs", "release-notes.md");
const BEGIN = "<!-- release-notes.cjs: begin. Generated from the release records: do not edit. -->";
const END = "<!-- release-notes.cjs: end -->";

const readJson = (file) => (file && fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, "utf8")) : undefined);
const relative = (file) => path.relative(root, file).replaceAll("\\", "/");

function latestEvidence(commit) {
  const directory = path.join(root, "audit", "release-evidence");
  if (!fs.existsSync(directory)) return undefined;
  const files = fs.readdirSync(directory).filter((name) => name.endsWith(".json") && (!commit || name.startsWith(commit.slice(0, 12)))).sort();
  return files.length === 0 ? undefined : path.join(directory, files.at(-1));
}

/** The files of the tarball `npm pack` would make, and its name. */
function packListing() {
  const text = execFileSync("npm", ["pack", "--dry-run", "--json", "--ignore-scripts"], { cwd: root, encoding: "utf8", shell: true, stdio: ["ignore", "pipe", "ignore"] });
  const listing = JSON.parse(text.slice(text.indexOf("[")))[0];
  return { tarball: listing.filename, files: listing.files.map((file) => file.path.replaceAll("\\", "/")) };
}

/** The generated block, from a matrix record, an evidence record and the pack listing. */
function block(record, recordFile, evidence, evidenceFile, pack, pkg) {
  const rows = evidence?.rows ?? [];
  const complete = rows.filter((row) => row.complete).map((row) => row.release);
  const units = record?.units ?? [];
  const passed = units.filter((unit) => unit.status === "pass").length;
  const lines = [
    BEGIN,
    "",
    "### Final pass result",
    "",
    record ? `From \`${relative(recordFile)}\` and \`${evidenceFile ? relative(evidenceFile) : "no evidence record"}\`. It counts what those records show, and nothing older.` : "No release matrix record exists, so there is no final pass result.",
    "",
    "| Item | Value |",
    "| --- | --- |",
    `| Commit and engine digest of the pass | ${record ? `\`${record.commit}\`, engine \`${record.engine.slice(0, 12)}\`` : "none"} |`,
    `| Matrix verdict | ${record ? `${record.verdict}${record.verdict === "green" ? "" : ` (${record.reasonCount} reason(s), \`docs/release-matrix.md\`)`}` : "none"} |`,
    `| Units passed | ${record ? `${passed} of ${units.length} run; ${record.scope.jobs.length} job(s) on ${record.scope.releases.length} release(s) asked for` : "none"} |`,
    `| Pinned corpus | ${record ? `${record.corpus.recordings} of ${record.corpus.pinned} recordings` : "none"} |`,
    `| Known failures | ${record ? record.knownFailures : "not read"} |`,
    `| Evidence rows complete | ${evidence ? `${complete.length} of ${rows.length} (\`docs/release-evidence.md\`)` : "none"} |`,
    `| Published list | ${evidence ? (evidence.publishedProblems.length === 0 ? "agrees with the rows" : `${evidence.publishedProblems.length} disagreement(s) with the rows`) : "not checked"} |`,
    "",
    `Releases supported by this candidate, each with a complete evidence row: ${complete.length === 0 ? "none." : `${complete.join(", ")}.`}`,
    "",
    "### Package as packed",
    "",
    "| Item | Value |",
    "| --- | --- |",
    `| Tarball | \`${pack.tarball}\` |`,
    `| Files | ${pack.files.length} |`,
    `| Public paths of package.json | ${matrix.publicPaths(pkg).map((entry) => `\`${entry}\``).join(", ")} |`,
    "",
    END,
  ];
  return lines.join("\n");
}

/**
 * Where the notes and the tarball disagree: the main entry the notes name must be package.json's, and every path the
 * Package table names, and every public path of package.json, must be in the tarball (a directory by some file under it).
 */
function notesPackProblems(notes, pkg, files) {
  const problems = [...matrix.exportProblems(pkg, files)];
  const listed = new Set(files);
  const row = (name) => new RegExp(`^\\| ${name} \\| (.*) \\|$`, "m").exec(notes)?.[1];
  const main = row("Main entry");
  if (!main) problems.push("the notes name no main entry");
  else if (!main.includes(`\`${pkg.main}\``)) problems.push(`the notes' main entry is ${main}, package.json's is ${pkg.main}`);
  for (const name of ["Main entry", "Other public path"]) {
    for (const [, entry] of (row(name) ?? "").matchAll(/`([^`]+)`/g)) {
      const found = listed.has(entry) || listed.has(`${entry}.js`) || [...listed].some((file) => file.startsWith(`${entry}/`));
      if (!found) problems.push(`${entry}, a public path of the notes, is not in the tarball`);
    }
  }
  return problems;
}

function main(args) {
  const commit = execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim();
  const recordFile = matrix.latestRecord(commit) ?? matrix.latestRecord();
  const record = readJson(recordFile);
  const evidenceFile = record ? latestEvidence(record.commit) : undefined;
  const evidence = readJson(evidenceFile);
  const pkg = readJson(path.join(root, "package.json"));
  const pack = packListing();
  const notes = fs.readFileSync(notesPath, "utf8").replaceAll("\r\n", "\n");
  const start = notes.indexOf(BEGIN);
  const end = notes.indexOf(END);
  if (start < 0 || end < start) throw new Error(`docs/release-notes.md has no generated block (${BEGIN} ... ${END})`);
  const generated = block(record, recordFile, evidence, evidenceFile, pack, pkg);
  const problems = notesPackProblems(notes, pkg, pack.files);
  if (args.includes("--check")) {
    if (notes.slice(start, end + END.length) !== generated) problems.push("the generated block is not what the records give: run node scripts/release-notes.cjs");
    for (const problem of problems) console.error(problem);
    return problems.length === 0 ? 0 : 1;
  }
  fs.writeFileSync(notesPath, `${notes.slice(0, start)}${generated}${notes.slice(end + END.length)}`);
  for (const problem of problems) console.error(problem);
  console.log(`docs/release-notes.md: ${record ? `from ${relative(recordFile)}` : "no record"}; ${problems.length} package problem(s)`);
  return problems.length === 0 ? 0 : 1;
}

module.exports = { notesPackProblems, block, BEGIN, END };

if (require.main === module) process.exitCode = main(process.argv.slice(2));
