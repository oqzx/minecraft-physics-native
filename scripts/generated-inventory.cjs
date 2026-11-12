// Every generated file the package or its build depends on, with the step that makes it and what that
// step reads, so nothing shipped comes from nowhere (docs/release-build.md). `pass` says how a run
// reaches it: `all` is a stage of scripts/generate-all.cjs, `network` and `oracle` are run by hand
// because their inputs are Mojang's live list or dumps and jars of running clients, and `build` is the
// native or TypeScript build. Checks that each tracked generated file is listed and each generator
// exists; running the generators is generate-all's --check. A `network` or `oracle` entry also says what pins its input
// (`pin`), what refuses a changed or wrong-release input before the output is taken (`detect`), and `capture`: `pins` when
// a host holding the pinned inputs makes it again, `captured` when the committed file is the record of a live input (a
// list, a running client, a recording, a sweep, an audit) taken at its pin, which no pin alone can make again.
const fs = require("node:fs");
const path = require("node:path");

const repositoryRoot = path.resolve(__dirname, "..");

const GENERATED = [
  { output: "generated/minecraft-versions.json", step: "scripts/generate-version-manifest.cjs", pass: "all", inputs: "exact release sources" },
  { output: "generated/block-registries.json", step: "scripts/generate-block-registry-manifest.cjs", pass: "all", inputs: "exact release sources" },
  { output: "generated/block-state-definitions.json", step: "scripts/generate-block-state-manifest.cjs", pass: "all", inputs: "exact release sources" },
  { output: "generated/legacy-release-data.json", step: "scripts/generate-legacy-release-manifest.cjs", pass: "all", inputs: "MC_LEGACY_SOURCES, the six legacy releases' decompiled jars" },
  { output: "native/data/", step: "scripts/generate-native-data.cjs", pass: "all", inputs: "the manifests above" },
  { output: "generated/block-friction.json", step: "scripts/generate-block-friction.cjs", pass: "all", inputs: "exact release sources" },
  { output: "generated/fluid-tables.json", step: "scripts/generate-fluid-tables.cjs", pass: "all", inputs: "exact release sources" },
  { output: "generated/physics-methods.json", step: "scripts/extract-physics-methods.cjs", pass: "all", inputs: "exact release sources" },
  { output: "generated/release-manifest.json", step: "scripts/generate-release-manifest.cjs", pass: "all", inputs: "the manifests above, generated/mojang-releases.json, generated/release-aliases.json" },
  { output: "generated/physics-breakpoints.json", step: "scripts/report-physics-breakpoints.cjs", pass: "all", inputs: "the manifests above, minecraft-data at the pin" },
  { output: "generated/block-tags.json", step: "scripts/generate-block-tags.cjs", pass: "all", inputs: "exact release sources" },
  { output: "generated/projectile-capabilities.json", step: "scripts/generate-projectile-capabilities.cjs", pass: "all", inputs: "exact release sources" },
  { output: "generated/block-shapes.json", step: "scripts/generate-block-shapes.cjs", pass: "all", inputs: "each release's pinned jar and libraries, run by a JDK" },
  { output: "generated/block-shapes", step: "scripts/generate-block-shapes.cjs", pass: "all", inputs: "each release's pinned jar and libraries, run by a JDK" },
  { output: "generated/entity-data.json", step: "scripts/generate-entity-data.cjs", pass: "all", inputs: "each release's pinned jar and libraries, run by a JDK" },
  { output: "generated/block-shapes-legacy.json", step: "scripts/generate-legacy-shapes.cjs", pass: "all", inputs: "the recorder's Mojang-named jars of 1.8.8 to 1.13.2" },
  { output: "generated/block-shapes-legacy", step: "scripts/generate-legacy-shapes.cjs", pass: "all", inputs: "the recorder's Mojang-named jars of 1.8.8 to 1.13.2" },
  { output: "generated/primitive-map.json", step: "scripts/generate-primitive-map.cjs", pass: "all", inputs: "exact release sources, native/src" },
  { output: "generated/movement-constants.json", step: "scripts/generate-movement-constants.cjs", pass: "all", inputs: "exact release sources, native/src, src/mineflayer" },
  { output: "generated/movement-sites.json", step: "scripts/generate-movement-sites.cjs", pass: "all", inputs: "exact release sources, MC_LEGACY_SOURCES, native/src, src/mineflayer, generated/movement-literal-types.json, scripts/movement-dispositions.cjs, audit/movement-site-reviews.json" },
  { output: "generated/movement-literal-types.json", step: "scripts/movement-literal-types.cjs", pass: "oracle", inputs: "rustc's typed tree of native/src",
    pin: "`source`: the sha256 of native/src's code, comments aside (codeHash), and `rustc`, the compiler that typed it (rust-toolchain.toml)",
    detect: "test/generated-inventories.test.cjs recomputes codeHash: a record of other code fails, a stale type with it; a literal with no type fails too",
    capture: "pins" },
  { output: "generated/provenance.json", step: "scripts/provenance.cjs", pass: "all", inputs: "the generated files and their generators" },
  { output: "generated/conformance-evidence.json", step: "scripts/pin-conformance.cjs of the recorder package (@minecraft-physics-native/recorder)", pass: "oracle", inputs: "recorded traces (REPLAY_RECORDINGS)",
    pin: "per pin: the hashes of its source record, tables and engine, a seal, the recordings index sha256 and the addon sha256",
    detect: "scripts/parity-contract.cjs pinProblems refuses a pin whose seal, scope, source record, tables or engine hash no longer match; a replayed recording of another release or protocol is refused by the recorder's replay/validate.ts. The recordings and addon hashes are recorded, not compared",
    capture: "captured" },
  { output: "generated/shared-gates.json", step: "scripts/shared-gates.cjs", pass: "oracle", inputs: "generated/gate-sources.json",
    pin: "none needed: native/src and generated/gate-sources.json are in the checkout",
    detect: "--check; test/shared-gates.test.cjs regenerates and compares",
    capture: "pins" },
  { output: "generated/gate-sources.json", step: "scripts/compare-gate-sources.cjs", pass: "oracle", inputs: "each release's Mojang-named jar through one Vineflower (--materialize), MC_LEGACY_SOURCES, audit/gate-source-reviews.json, native/src",
    pin: "DECOMPILER (Vineflower 1.12.0 by sha1) and the Mojang-named jars of scripts/source-pins.cjs",
    detect: "--materialize takes each jar only at its pin (NAMED_JARS sha256, or the client sha1) and Vineflower only at its sha1; the compare cache it made is not hashed afterwards, and --check needs that cache, so it fails on a host without it",
    capture: "pins" },
  { output: "generated/shared-equivalence.json", step: "scripts/shared-equivalence.cjs", pass: "oracle", inputs: "the compare trees of scripts/compare-gate-sources.cjs (--materialize), MC_LEGACY_SOURCES, audit/shared-equivalence-reviews.json, native/src",
    pin: "the compare trees' pins, as for generated/gate-sources.json",
    detect: "--check needs the compare cache and fails without it; test/shared-equivalence.test.cjs holds the record's functions and rules to native/src with no trees",
    capture: "pins" },
  { output: "generated/shared-reach.json", step: "scripts/reach-probe.cjs (prepare, tests, collect; the corpus step is the recorder package's scripts/reach-corpus.cjs)", pass: "oracle", inputs: "a probe copy of native/ (scripts/gate-mutants.cjs), its Rust tests, and the recorder's replay/gate-sweep.ts over its corpus of every release",
    pin: "`probedAt`: the commit whose native/src the probe copy was made from",
    detect: "collect refuses a release with no finished corpus replay; test/shared-equivalence.test.cjs holds every change put down to reach to the record's releases",
    capture: "captured" },
  { output: "generated/gate-mutation.json", step: "scripts/gate-report.cjs", pass: "oracle", inputs: "a sweep of scripts/gate-mutants.cjs and the recorder's replay/gate-sweep.ts",
    pin: "`commit`: the sweep's gates are native/src's at that commit (scripts/gate-mutants.cjs gateSitesAt)",
    detect: "gate-report refuses a sweep without --commit, or whose gates are not that commit's (sweepProblems); test/shared-gates.test.cjs holds the committed record to its commit",
    capture: "captured" },
  { output: "generated/subsystem-matrix.json", step: "replay/subsystem-matrix.ts of the recorder package (@minecraft-physics-native/recorder)", pass: "oracle", inputs: "runs of the recorder's replay/cli.ts over REPLAY_RECORDINGS (--replays, --recheck), generated/shared-gates.json, the recorder's corpus and scenario modules",
    pin: "`runs`: each replay run's commit and addon sha256, per mode, and the recheck's",
    detect: "--check makes the source, corpus and verdicts again from the kept replay summary and fails on a difference (the recorder's test/subsystem-matrix.test.cjs); a recording of another release or protocol is refused by the recorder's replay/validate.ts",
    capture: "captured" },
  { output: "generated/tag-audit-dispositions.json", step: "scripts/classify-tag-audit.cjs", pass: "oracle", inputs: "generated/block-tags.json, reviewed",
    pin: "AUDIT: the audit file's sha256 (E:/mc-physics-demo/audit/tag-differences.json on the host that made it); each client jar at its pinned sha1",
    detect: "classify-tag-audit refuses an audit of other bytes (auditProblem; --accept-audit after review) and a jar off its pin before classifying; test/block-tags.test.cjs holds the record to AUDIT",
    capture: "captured" },
  { output: "generated/mojang-releases.json", step: "scripts/generate-mojang-releases.cjs", pass: "network", inputs: "Mojang's version manifest",
    pin: "the sha1 of each release's version json: the root of the jar pins (scripts/fetch-jar-inputs.cjs)",
    detect: "a run fails before writing when the list gives a registered release another version json sha1, or drops it (pinChanges; --accept-changed after review); a fetched version json and mappings are held to the sha1 the list gives them",
    capture: "captured" },
  { output: "generated/runtime-registries.json", step: "scripts/generate-runtime-registries.cjs", pass: "oracle", inputs: "MC_PHYSICS_DATA_GENERATOR client dumps",
    pin: "dumpSha256 of each client dump",
    detect: "a dump whose runtime version is another release is refused; generate-release-manifest.cjs holds the protocol to minecraft-data; test/block-registries.test.cjs compares tables to dumps",
    capture: "captured" },
  { output: "generated/release-aliases.json", step: "scripts/generate-release-aliases.cjs", pass: "oracle", inputs: "client jars of the aliased releases",
    pin: "ALIASES: the client sha1 of both releases; the remapped jars by sha256",
    detect: "the client sha1 is checked against ALIASES before comparing; --check (CI, and test/release-aliases.test.cjs where the jars are) repeats the comparison against the recorded sha256",
    capture: "pins" },
  { output: "generated/data-aliases.json", step: "scripts/generate-data-aliases.cjs", pass: "oracle", inputs: "client and server jars, the JDK of MC_JAVA, minecraft-data",
    pin: "`jars`: each client by its pinned sha1, each server by the sha1 its pinned version json gives, each intermediary remap by the sha256 recorded",
    detect: "jarIdentity throws on a jar off its pin before anything is compared; the jar's en_us.json must equal minecraft-data's, and minecraft-data's version and protocol must match; --check repeats it",
    capture: "pins" },
  { output: "src/mineflayer/flow-blockers.json", step: "scripts/generate-flow-blockers.cjs", pass: "oracle", inputs: "exact release sources",
    pin: "the exact Mojang-named trees of scripts/source-tree-pins.json (cache:, legacy:1.13.2, mc-source:26.2)",
    detect: "scripts/bootstrap-inputs.cjs and materialize-sources.cjs hold those trees to their pins; --check regenerates and compares",
    capture: "pins" },
  { output: "src/mineflayer/vehicle-versions.ts", step: "scripts/generate-vehicle-engines.cjs", pass: "oracle", inputs: "native/src/versioned",
    pin: "none needed: native/src/versioned is in the checkout",
    detect: "test/vehicle-engines.test.cjs regenerates and compares",
    capture: "pins" },
  { output: "native/artifacts/index.*.node", step: "napi build", pass: "build", inputs: "native/, native/Cargo.lock, rust-toolchain.toml" },
  { output: "native/artifacts/index.wasi-browser.js", step: "scripts/patch-wasi-loader.cjs", pass: "build", inputs: "napi build's loader" },
  { output: "native/artifacts/index.wasip1-browser.js", step: "scripts/patch-wasi-loader.cjs", pass: "build", inputs: "napi build's loader" },
  { output: "native/artifacts/*.wasm", step: "napi build --target wasm32-*", pass: "build", inputs: "native/, native/Cargo.lock, emnapi" },
  { output: "native/artifacts/native.js", step: "napi build", pass: "build", inputs: "package.json napi" },
  { output: "native/artifacts/native.d.ts", step: "napi build", pass: "build", inputs: "the #[napi] items of native/src" },
  { output: "native/artifacts/provenance.*.json", step: "scripts/release-build.cjs", pass: "build", inputs: "the build it records" },
  { output: "dist/", step: "tsc", pass: "build", inputs: "src/, tsconfig.json, pnpm-lock.yaml" },
];

/** What is unaccounted for: `tracked` are the files of generated/, `exists` says whether a path does. */
function inventoryGaps(tracked, exists) {
  const gaps = [];
  const outputs = new Set(GENERATED.map(({ output }) => output));
  for (const file of tracked) if (!outputs.has(file)) gaps.push(`${file} has no generator on record`);
  for (const { output, step, pass, pin, detect, capture } of GENERATED) {
    if ((pass === "network" || pass === "oracle") && !(pin && detect)) gaps.push(`${output} is made by hand and does not say what pins or checks its input`);
    if ((pass === "network" || pass === "oracle") && !["pins", "captured"].includes(capture)) gaps.push(`${output} does not say whether pins make it again or it is a captured input`);
    if (/^none\b/.test(detect ?? "")) gaps.push(`${output}: nothing detects a changed input`);
    if (step.endsWith(".cjs") || step.endsWith(".ts")) {
      if (!exists(step)) gaps.push(`${output} names ${step}, which does not exist`);
    }
  }
  return gaps;
}

if (require.main === module) {
  const tracked = fs.readdirSync(path.join(repositoryRoot, "generated")).map((name) => `generated/${name}`);
  const gaps = inventoryGaps(tracked, (file) => fs.existsSync(path.join(repositoryRoot, file)));
  for (const gap of gaps) console.error(gap);
  if (gaps.length > 0) process.exit(1);
  console.log(`generated inventory: ${GENERATED.length} outputs, each with its step`);
}

module.exports = { GENERATED, inventoryGaps };
