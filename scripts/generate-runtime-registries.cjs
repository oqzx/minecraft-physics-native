// Writes generated/runtime-registries.json: what each release's running client reports, read from
// the GameplayRegistryProbe dumps under MC_PHYSICS_DATA_GENERATOR (build/<release>-gameplay-registry.json).
// A dump is the oracle the source extractions are held to: every block name, its number of states
// and the offset of its default state, with the version and protocol the client itself reported.
// A dump that names another release is refused. A release without a dump keeps its entry only if
// it had one; the others stay absent, which the checks report.
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { registeredReleases } = require("./registered-releases.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.join(repositoryRoot, "generated", "runtime-registries.json");
const generatorRoot = process.env.MC_PHYSICS_DATA_GENERATOR ?? "D:/minecraft/java/physics-data-generator";

/** The entry of a probe's output, which the launcher's log prefix precedes. */
function readDump(text, release) {
  const json = text.slice(text.indexOf('{"'));
  const dump = JSON.parse(json.slice(0, json.lastIndexOf("}") + 1));
  if (dump.runtimeGameVersion !== release) {
    throw new Error(`The runtime dump of ${release} is from ${dump.runtimeGameVersion}`);
  }
  const blocks = [...dump.blocks]
    .sort((left, right) => left.id - right.id)
    .map((block) => [block.name, block.maxStateId - block.minStateId + 1, block.defaultState - block.minStateId]);
  return {
    runtimeGameVersion: dump.runtimeGameVersion,
    runtimeProtocolVersion: dump.runtimeProtocolVersion,
    dumpSha256: crypto.createHash("sha256").update(text).digest("hex"),
    blocks,
  };
}

function main() {
  const previous = fs.existsSync(outputPath) ? JSON.parse(fs.readFileSync(outputPath, "utf8")).releases : {};
  const releases = {};
  for (const { version } of registeredReleases()) {
    const file = path.join(generatorRoot, "build", `${version}-gameplay-registry.json`);
    if (fs.existsSync(file)) releases[version] = readDump(fs.readFileSync(file, "utf8"), version);
    else if (previous[version]) releases[version] = previous[version];
  }
  const lines = Object.entries(releases).map(([version, entry]) => `    ${JSON.stringify(version)}: ${JSON.stringify(entry)}`);
  fs.writeFileSync(outputPath, `{\n  "schema": 1,\n  "generatedBy": "scripts/generate-runtime-registries.cjs",\n  "releases": {\n${lines.join(",\n")}\n  }\n}\n`);
  console.log(`Runtime registries of ${lines.length} releases.`);
}

if (require.main === module) main();

module.exports = { readDump };
