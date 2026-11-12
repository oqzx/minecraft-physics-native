// Runs every generator in dependency order as one step: a stage that fails puts the whole previous
// artifact set back, so a run publishes all of it or none of it. With --check nothing is kept and a
// difference from the checked-in artifacts (line endings aside) fails. With --cached a release whose
// sources are absent keeps its entries, which is what such a run must say it is doing; without it
// that release fails the run, and the run ends by saying which it was (`generation: fresh` or `cached`).
//
// A run killed outright (a closed console, TerminateProcess, a power cut) cannot put anything back, so
// it leaves a journal naming its snapshot of the previous set; the next run, or `--recover`, restores
// that set before anything else.
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const repositoryRoot = path.resolve(__dirname, "..");
const artifacts = [path.join(repositoryRoot, "generated"), path.join(repositoryRoot, "native", "data")];
const cachedStages = new Set(["generate-legacy-release-manifest.cjs", "generate-block-registry-manifest.cjs", "generate-block-state-manifest.cjs", "generate-version-manifest.cjs", "extract-physics-methods.cjs", "generate-block-tags.cjs", "generate-block-shapes.cjs", "generate-entity-data.cjs", "report-physics-breakpoints.cjs", "generate-primitive-map.cjs", "generate-movement-sites.cjs"]);
const stages = [
  "generate-version-manifest.cjs",
  "generate-block-registry-manifest.cjs",
  "generate-block-state-manifest.cjs",
  "generate-legacy-release-manifest.cjs",
  "generate-native-data.cjs",
  "generate-block-tags.cjs",
  "generate-block-shapes.cjs",
  "generate-legacy-shapes.cjs",
  "generate-entity-data.cjs",
  "generate-block-friction.cjs",
  "generate-fluid-tables.cjs",
  "generate-projectile-capabilities.cjs",
  "generate-primitive-map.cjs",
  "extract-physics-methods.cjs",
  "generate-release-manifest.cjs",
  "report-physics-breakpoints.cjs",
  "generate-movement-constants.cjs",
  "generate-movement-sites.cjs",
  "provenance.cjs",
  "verify-generation-invariants.cjs",
];

function filesUnder(directory, base = directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(directory, entry.name);
    return entry.isDirectory() ? filesUnder(file, base) : [path.relative(base, file)];
  });
}

const normalized = (file) => fs.readFileSync(file).toString("latin1").replaceAll("\r\n", "\n");

/** Where a run of this checkout notes its snapshot while it runs: one per checkout, outside it. */
const defaultJournal = (cwd = repositoryRoot) => path.join(os.tmpdir(), `mc-generate-all-${crypto.createHash("sha1").update(path.resolve(cwd)).digest("hex").slice(0, 12)}.json`);

function restoreFrom(copies) {
  for (const [folder, copy] of copies) {
    fs.rmSync(folder, { recursive: true, force: true });
    fs.cpSync(copy, folder, { recursive: true });
  }
}

/**
 * Puts back the artifact set a killed run left its snapshot of, then forgets it: `true` if there was one. A journal
 * is written only once its snapshot is whole, so one that exists can always be restored from.
 */
function recover(journal = defaultJournal()) {
  if (!fs.existsSync(journal)) return false;
  const { snapshot, copies } = JSON.parse(fs.readFileSync(journal, "utf8"));
  restoreFrom(copies);
  fs.rmSync(journal, { force: true });
  fs.rmSync(snapshot, { recursive: true, force: true });
  return true;
}

/**
 * Runs `stages` over the artifact folders and says how it went: `{ failed, drift, recovered }`, the folders back as they
 * were if a stage failed, the run was interrupted or this is a check.
 */
function runStages({ stages: names, folders, check = false, cached = false, directory = __dirname, cwd = repositoryRoot, journal = defaultJournal(cwd) }) {
  const recovered = recover(journal);
  const snapshot = fs.mkdtempSync(path.join(os.tmpdir(), "mc-generated-"));
  const copies = folders.map((folder, index) => [folder, path.join(snapshot, String(index))]);
  for (const [folder, copy] of copies) fs.cpSync(folder, copy, { recursive: true });
  fs.writeFileSync(`${journal}.partial`, JSON.stringify({ snapshot, copies }));
  fs.renameSync(`${journal}.partial`, journal);
  const forget = () => {
    fs.rmSync(journal, { force: true });
    fs.rmSync(snapshot, { recursive: true, force: true });
  };
  // (an interrupt the process lives to see: put the set back at once rather than at the next run)
  const interrupted = (signal) => {
    restoreFrom(copies);
    forget();
    console.error(`generation interrupted by ${signal}: the previous artifacts are back in place`);
    process.exit(130);
  };
  const signals = ["SIGINT", "SIGTERM", "SIGHUP"];
  for (const signal of signals) process.on(signal, interrupted);
  let failed = null;
  try {
    for (const stage of names) {
      const args = [path.join(directory, stage), ...(cached && cachedStages.has(stage) ? ["--cached"] : [])];
      const result = spawnSync(process.execPath, args, { stdio: "inherit", cwd });
      if (result.status !== 0) {
        failed = result.signal ? `${stage} (${result.signal})` : stage;
        break;
      }
    }
  } finally {
    for (const signal of signals) process.off(signal, interrupted);
  }
  const drift = [];
  if (!failed && check) {
    for (const [folder, copy] of copies) {
      const before = new Set(filesUnder(copy));
      for (const file of filesUnder(folder)) {
        if (!before.delete(file) || normalized(path.join(folder, file)) !== normalized(path.join(copy, file))) drift.push(file);
      }
      drift.push(...before);
    }
  }
  if (failed || check) restoreFrom(copies);
  forget();
  return { failed, drift, recovered };
}

function main() {
  if (process.argv.includes("--recover")) {
    console.log(recover() ? "an interrupted generation's previous artifacts are back in place" : "no interrupted generation to recover");
    return;
  }
  const cached = process.argv.includes("--cached");
  const { failed, drift, recovered } = runStages({ stages, folders: artifacts, check: process.argv.includes("--check"), cached });
  if (recovered) console.log("an interrupted generation was found: its previous artifacts were put back before this run");
  if (failed) {
    console.error(`${failed} failed: the previous artifacts are back in place`);
    process.exit(1);
  }
  if (drift.length > 0) {
    console.error(`Regeneration differs from the checked-in artifacts: ${drift.join(", ")}`);
    process.exit(1);
  }
  // what kind of evidence this run is: a cached run may have kept tables whose inputs were not at hand
  if (!cached) {
    console.log("generation: fresh (every release from its exact inputs)");
    return;
  }
  const absent = require("./bootstrap-inputs.cjs").inputProblems();
  const kept = absent.map((problem) => `\n  ${problem}`).join("");
  console.log(`generation: cached (${absent.length === 0 ? "every input was at hand, so nothing was kept" : `what these inputs feed was kept as committed:${kept}`})`);
}

if (require.main === module) main();

module.exports = { runStages, recover, defaultJournal };
