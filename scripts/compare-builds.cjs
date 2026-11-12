// The reproducibility policy, applied to the reports of two runs of scripts/release-build.cjs
// (docs/release-build.md):
//   node scripts/compare-builds.cjs <a.report.json> <b.report.json>
// Inputs must be equal whatever the host: the sources, tables, locks and pins, line endings aside.
// Two runs of one target must also give the same bytes for every shipped file, and the same tarball
// contents. Runs of different targets share only the compiled adapter's inputs: their binaries are
// different artifacts, which is why each has its own record.
const assert = require("node:assert/strict");
const fs = require("node:fs");

/** What two records break of the policy. */
function comparisonProblems(left, right) {
  const problems = [];
  for (const key of new Set([...Object.keys(left.inputs), ...Object.keys(right.inputs)])) {
    if (left.inputs[key] !== right.inputs[key]) problems.push(`inputs.${key} differs: ${left.inputs[key]} and ${right.inputs[key]}`);
  }
  if (left.target !== right.target) return problems;
  if (left.dataGeneration !== right.dataGeneration) problems.push(`one run regenerated ${left.dataGeneration}, the other ${right.dataGeneration}`);
  for (const key of ["rustc", "cargo", "napiCli", "typescript"]) {
    if (left.toolchain[key] !== right.toolchain[key]) problems.push(`toolchain.${key} differs: ${left.toolchain[key]} and ${right.toolchain[key]}`);
  }
  for (const file of new Set([...Object.keys(left.files), ...Object.keys(right.files)])) {
    if (left.files[file] !== right.files[file]) problems.push(`${file} differs between two builds of ${left.target}`);
  }
  return problems;
}

if (require.main === module) {
  const [left, right] = process.argv.slice(2, 4).map((file) => JSON.parse(fs.readFileSync(file, "utf8")));
  assert.equal(left.records.length, right.records.length, "the runs built different target lists");
  const problems = left.records.flatMap((record, index) => comparisonProblems(record, right.records[index]));
  for (const problem of problems) console.error(problem);
  if (problems.length > 0) process.exit(1);
  console.log(`equal under the policy: ${left.records.map(({ target }) => target).join(", ")}`);
}

module.exports = { comparisonProblems };
