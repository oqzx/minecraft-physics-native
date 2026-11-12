// What a registered release has shown, one dimension at a time. Registering a release dispatches it;
// it says nothing of parity. Each dimension is computed from pinned evidence and carries its reasons,
// so a stale table or a changed alias fails that dimension and no other, and no flag can be set by
// hand. See docs/parity-contract.md.
//   source          the record is the release's own and can be reproduced (scripts/source-admission.cjs)
//   data            the generated tables are the ones joined, and hold what the running client holds
//   implementation  every interaction of the contract has its entry points and no declared gap
//   conformance     every cell of the contract has a pinned, current, complete and matching trace
//
//   node scripts/release-admission.cjs            every release, one line each
//   node scripts/release-admission.cjs 26.1.2     that release; exits 1 unless all dimensions pass
//   node scripts/release-admission.cjs --write    writes src/release-status.json for the package
//   node scripts/release-admission.cjs --check    fails when that file differs from what this finds
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { aliasDefect, evidenceHash } = require("./generate-release-aliases.cjs");
const { INTERACTIONS, contractCells, engineDigest } = require("./parity-contract.cjs");
const { sourceBlocker, sourceRecordProblems } = require("./source-admission.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const statusPath = path.join(repositoryRoot, "src", "release-status.json");
const evidencePath = path.join(repositoryRoot, "generated", "conformance-evidence.json");
const readGenerated = (name) => JSON.parse(fs.readFileSync(path.join(repositoryRoot, "generated", name), "utf8"));

function sourceStatus(version, record, alias) {
  const { source } = record;
  const problems = sourceRecordProblems(version, record);
  if (source.status === "alias") {
    const defect = aliasDefect(alias);
    if (defect) return { status: "rejected", reasons: [`alias: ${defect}`] };
    if (evidenceHash(alias) !== source.evidence) return { status: "rejected", reasons: ["alias: the evidence changed since the manifest was joined"] };
    if (problems.length > 0) return { status: "rejected", reasons: problems };
    return { status: "alias", reasons: [`served from ${source.sourceRelease}`] };
  }
  if (source.status !== "embedded") return { status: "missing", reasons: [`source is ${source.status}`, ...problems] };
  if (problems.length > 0) return { status: "rejected", reasons: problems };
  const blocker = sourceBlocker(record);
  return blocker ? { status: "blocked", reasons: [blocker] } : { status: "exact", reasons: [] };
}

function dataStatus(version, record, inputs) {
  const reasons = [];
  if (inputs.moduleSha256 !== record.tables.sha256) reasons.push("the table is not the one the manifest joined");
  const runtime = inputs.runtime;
  const blocks = inputs.registry?.blocks;
  if (runtime && blocks) {
    const names = runtime.blocks.map(([id]) => id);
    const absent = names.filter((id) => !blocks.includes(id));
    const extra = blocks.filter((id) => !names.includes(id));
    if (absent.length > 0) reasons.push(`${absent.length} blocks of the client are not in the table`);
    if (extra.length > 0) reasons.push(`${extra.length} blocks in the table are not the client's`);
    if ((inputs.registry.unresolvedClassCount ?? 0) > 0) reasons.push(`${inputs.registry.unresolvedClassCount} blocks have no class`);
  } else if (!inputs.legacy) {
    reasons.push("no running-client dump to hold the registry to");
  }
  if (inputs.dynamicBlocks?.length > 0) reasons.push(`state_index refuses ${inputs.dynamicBlocks.length} dynamic block classes: ${inputs.dynamicBlocks.join(", ")}`);
  return { status: reasons.length === 0 ? "complete" : "incomplete", reasons };
}

/** The contract's cells, as the two dimensions that read them. */
function contractStatus(cells, interactions, entryPoints) {
  const required = Object.entries(cells).filter(([, cell]) => cell.status !== "unavailable");
  const gaps = required.flatMap(([id, cell]) => cell.reasons.filter((reason) => reason.startsWith("gap")).map((reason) => `${id}: ${reason}`));
  const unserved = interactions.flatMap((interaction) => (cells[interaction.id].status === "unavailable" ? []
    : interaction.api.filter((name) => entryPoints && !entryPoints.has(name)).map((name) => `${interaction.id}: no entry point ${name}`)));
  const implementation = { status: gaps.length + unserved.length === 0 ? "complete" : "incomplete", reasons: [...unserved, ...gaps] };

  const unpinned = (cell) => cell.reasons.at(-1) === "no pinned trace evidence";
  const verified = required.filter(([, cell]) => cell.status === "verified");
  const pinned = required.filter(([, cell]) => !unpinned(cell));
  const pending = required.filter(([, cell]) => cell.status !== "verified");
  const status = pending.length === 0 && required.length > 0 ? "verified" : pinned.length > 0 ? "partial" : "unverified";
  const reasons = pinned.length === 0 ? ["no pinned trace evidence is recorded for the release"]
    : pending.length === 0 ? [] : [`${verified.length} of ${required.length} cells verified`,
      ...pending.filter(([, cell]) => !unpinned(cell)).map(([id, cell]) => `${id}: ${cell.reasons.at(-1)}`)];
  return { implementation, conformance: { status, reasons } };
}

/** The admission of one release from its joined record and what it was joined from. */
function admission(version, record, inputs) {
  const identity = {
    source: crypto.createHash("sha256").update(JSON.stringify(record.source)).digest("hex"),
    tables: record.tables.sha256,
    engine: inputs.engine,
  };
  const interactions = inputs.interactions ?? INTERACTIONS;
  const cells = contractCells(version, inputs.pins, identity, interactions);
  return {
    source: sourceStatus(version, record, inputs.alias),
    data: dataStatus(version, record, inputs),
    ...contractStatus(cells, interactions, inputs.entryPoints),
  };
}

const admitted = (status) => ["exact", "alias"].includes(status.source.status) && status.data.status === "complete"
  && status.implementation.status === "complete" && status.conformance.status === "verified";

/** Every name the package exports, for holding the contract's entry points to something real. */
function exportedNames() {
  const names = new Set();
  const read = (file) => fs.readFileSync(path.join(repositoryRoot, file), "utf8");
  for (const [, name] of read("native/artifacts/native.d.ts").matchAll(/^export declare (?:function|class) (\w+)/gm)) names.add(name);
  const walk = (directory) => fs.readdirSync(path.join(repositoryRoot, directory), { withFileTypes: true }).flatMap((entry) => (
    entry.isDirectory() ? walk(`${directory}/${entry.name}`) : entry.name.endsWith(".ts") ? [`${directory}/${entry.name}`] : []));
  for (const file of walk("src")) for (const [, name] of read(file).matchAll(/^export (?:abstract )?(?:class|function|const) (\w+)/gm)) names.add(name);
  return names;
}

function allAdmissions() {
  const { sha256OfModule } = require("./generate-release-manifest.cjs");
  const manifest = readGenerated("release-manifest.json").releases;
  const runtime = readGenerated("runtime-registries.json").releases;
  const registries = readGenerated("block-registries.json").versions;
  const states = readGenerated("block-state-definitions.json").releases;
  const aliases = readGenerated("release-aliases.json").aliases;
  const pins = fs.existsSync(evidencePath) ? JSON.parse(fs.readFileSync(evidencePath, "utf8")).releases : {};
  const engine = engineDigest();
  const entryPoints = exportedNames();
  return Object.fromEntries(Object.keys(manifest).map((version) => {
    const dynamicBlocks = Object.entries(states[version]?.blocks ?? {})
      .filter(([, block]) => block.unresolvedProperties.includes("<dynamic>"))
      .map(([className]) => className);
    return [version, admission(version, manifest[version], {
      alias: aliases[version],
      moduleSha256: sha256OfModule(version),
      runtime: runtime[version],
      registry: registries[version],
      legacy: !registries[version],
      dynamicBlocks,
      pins,
      engine,
      entryPoints,
    })];
  }));
}

/**
 * The status the package ships. Trace evidence ages with every native change, so what ships of it is
 * only whether a release is admitted, which a stale pin can take away and --check then refuses.
 */
function statusDocument(all = allAdmissions()) {
  const dimension = ({ status, reasons }) => ({ status, reasons });
  return {
    schema: 1,
    generatedBy: "scripts/release-admission.cjs --write",
    releases: Object.fromEntries(Object.entries(all).map(([version, status]) => [version, {
      source: dimension(status.source),
      data: dimension(status.data),
      implementation: dimension(status.implementation),
      admitted: admitted(status),
    }])),
  };
}

/** Where the shipped status differs from what the evidence supports. */
function statusProblems(shipped, all = allAdmissions()) {
  const expected = statusDocument(all).releases;
  const problems = [];
  for (const version of new Set([...Object.keys(expected), ...Object.keys(shipped.releases)])) {
    if (JSON.stringify(shipped.releases[version]) !== JSON.stringify(expected[version])) problems.push(`src/release-status.json is stale for ${version}`);
  }
  return problems;
}

if (require.main === module) {
  const all = allAdmissions();
  if (process.argv.includes("--write")) {
    fs.writeFileSync(statusPath, `${JSON.stringify(statusDocument(all), null, 2)}\n`);
    process.exit(0);
  }
  if (process.argv.includes("--check")) {
    const problems = statusProblems(JSON.parse(fs.readFileSync(statusPath, "utf8")), all);
    for (const problem of problems) console.error(problem);
    process.exit(problems.length > 0 ? 1 : 0);
  }
  const wanted = process.argv[2];
  if (wanted && !all[wanted]) {
    console.error(`${wanted} is not a registered release`);
    process.exit(2);
  }
  for (const [version, status] of Object.entries(wanted ? { [wanted]: all[wanted] } : all)) {
    console.log(`${version}: source ${status.source.status}, data ${status.data.status}, implementation ${status.implementation.status}, conformance ${status.conformance.status}`);
    if (wanted) for (const dimension of Object.values(status)) for (const reason of dimension.reasons) console.log(`  ${reason}`);
  }
  if (wanted && !admitted(all[wanted])) process.exit(1);
}

module.exports = { admission, admitted, allAdmissions, exportedNames, statusDocument, statusProblems };
