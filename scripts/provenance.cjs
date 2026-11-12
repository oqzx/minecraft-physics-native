// Where a generated entry comes from, step by step, and whether the generated files are still what produced them.
//
// `explain(kind, release, key)` answers for one entry: an id, a property, a tag membership, a constant, a shape or a
// friction. Each step names the generator and the input, by hash, that it read. It keeps two answers apart: `identity`,
// whether the input is the release's, and `behaviour`, which no table can answer and the replays do.
//
// generated/provenance.json holds the hash of every generated file and generator, so a table edited by hand, a generator
// changed without a run, or a run over other inputs is found by content, never by a timestamp.
//   node scripts/provenance.cjs              writes it
//   node scripts/provenance.cjs --check      fails on any difference
//   node scripts/provenance.cjs tag 26.3 doors    one entry's chain
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const zlib = require("node:zlib");

const repositoryRoot = path.resolve(__dirname, "..");

/** The Java a generator runs: its answers are the table's, so a change to it is a change to the generator. */
const ORACLES = ["java-oracle/ShapeDump.java", "java-oracle/EntityDump.java", "java-oracle/LiveEntityDump.java", "java-oracle/LegacyEntityDump.java", "java-oracle/LegacyShapeDump.java"];

/** Each generated file and the generator that writes it. */
const OUTPUTS = {
  "block-registries.json": "generate-block-registry-manifest.cjs",
  "block-state-definitions.json": "generate-block-state-manifest.cjs",
  "block-tags.json": "generate-block-tags.cjs",
  "block-friction.json": "generate-block-friction.cjs",
  "block-shapes.json": "generate-block-shapes.cjs",
  "block-shapes-legacy.json": "generate-legacy-shapes.cjs",
  "entity-data.json": "generate-entity-data.cjs",
  "fluid-tables.json": "generate-fluid-tables.cjs",
  "legacy-release-data.json": "generate-legacy-release-manifest.cjs",
  "minecraft-versions.json": "generate-version-manifest.cjs",
  "movement-constants.json": "generate-movement-constants.cjs",
  "movement-literal-types.json": "movement-literal-types.cjs",
  "movement-sites.json": "generate-movement-sites.cjs",
  "physics-methods.json": "extract-physics-methods.cjs",
  "primitive-map.json": "generate-primitive-map.cjs",
  "projectile-capabilities.json": "generate-projectile-capabilities.cjs",
  "release-manifest.json": "generate-release-manifest.cjs",
  "runtime-registries.json": "generate-runtime-registries.cjs",
};

const sha256 = (data) => crypto.createHash("sha256").update(data).digest("hex");
/** A text file's hash with its line endings aside, which git decides. */
const textHash = (file) => sha256(fs.readFileSync(file, "latin1").replaceAll("\r\n", "\n"));

function reader(root) {
  const cache = new Map();
  return (name) => {
    if (!cache.has(name)) cache.set(name, JSON.parse(fs.readFileSync(path.join(root, name), "utf8")));
    return cache.get(name);
  };
}

/** The identity of a release's sources, apart from anything about its behaviour. */
function identityOf(read, release) {
  const record = read("release-manifest.json").releases[release];
  if (!record) throw new Error(`${release} is not a registered release`);
  const { status } = record.source;
  if (status !== "embedded" && status !== "alias") throw new Error(`${release}: its source is ${status}, nothing can be traced to it`);
  return { status, embedded: record.source.embedded, mapping: record.source.mapping ?? record.mappings?.kind, versionJsonSha1: record.versionJsonSha1 };
}

const step = (generator, input, sha, detail) => ({ generator, input, sha256: sha, ...(detail ? { detail } : {}) });

const CHAINS = {
  block(read, release, id) {
    const registry = read("block-registries.json").versions[release];
    const legacy = read("legacy-release-data.json").releases[release];
    if (legacy) {
      if (!legacy.registry.blocks.includes(id)) throw new Error(`${id} is not a block of ${release}`);
      return [step("generate-legacy-release-manifest.cjs", legacy.registry.source, legacy.registry.sha256, legacy.registry.classes[id])];
    }
    const entry = registry?.classes[id];
    if (!entry) throw new Error(`${id} is not a block of ${release}`);
    const runtime = read("runtime-registries.json").releases[release];
    return [
      step("generate-block-registry-manifest.cjs", registry.source, registry.sha256, `${entry.field}: ${entry.className}`),
      step("generate-runtime-registries.cjs", `client dump of ${runtime.runtimeGameVersion}`, runtime.dumpSha256, "held equal to the registry"),
    ];
  },

  property(read, release, key) {
    const [id, name] = key.split("|");
    const chain = CHAINS.block(read, release, id);
    const className = read("block-registries.json").versions[release]?.classes[id]?.className;
    const definition = read("block-state-definitions.json").releases[release]?.blocks[className];
    if (!definition?.properties?.[name]) throw new Error(`${className} of ${release} declares no property ${name}`);
    return [...chain, step("generate-block-state-manifest.cjs", definition.source, definition.sha256, `${className}.${name}`)];
  },

  tag(read, release, key, root) {
    const [id, block] = key.split("|");
    const tags = read("block-tags.json").releases[release];
    if (!tags?.tags) throw new Error(`${release} has no tags: it decides by predicates`);
    const file = tags.default[id];
    if (!file) {
      if (tags.declaredEmpty[id]) return [step("generate-block-tags.cjs", `client ${tags.client}`, tags.client, `#${id} is declared with no file: empty by the data`)];
      throw new Error(`#${id} is no tag of ${release}`);
    }
    const chain = [step("generate-block-tags.cjs", `client ${tags.client}: data/minecraft/tags/block/${id}.json`, file.file, `membership ${file.membership}`)];
    if (!block) return chain;
    const module = fs.readFileSync(path.join(root, "..", "native", "data", "releases", `v${release.replaceAll(".", "_")}.rs`), "utf8");
    const lists = new Map([...module.matchAll(/pub static (\w+): &\[&str\] = &\[(.*?)\];/g)].map((match) => [match[1], [...match[2].matchAll(/"([^"]*)"/g)].map((name) => name[1])]));
    const rows = new Map([...module.matchAll(/GeneratedBlockTag \{ id: "([^"]+)".*?blocks: (\w+), includes: (\w+)/g)].map((match) => [match[1], { blocks: lists.get(match[2]) ?? [], includes: lists.get(match[3]) ?? [] }]));
    const path_ = [];
    const visit = (tag, seen) => {
      const row = rows.get(tag);
      if (!row || seen.has(tag)) return false;
      if (row.blocks.includes(block)) {
        path_.push(tag);
        return true;
      }
      for (const included of row.includes) {
        if (visit(included, new Set([...seen, tag]))) {
          path_.push(tag);
          return true;
        }
      }
      return false;
    };
    if (!visit(id, new Set())) throw new Error(`${block} is not in #${id} of ${release}`);
    return [...path_.reverse().slice(1).map((tag) => step("generate-block-tags.cjs", `client ${tags.client}: #${tag}`, tags.default[tag]?.file ?? null, "by inclusion")), ...chain, step("generate-native-data.cjs", `v${release}.rs`, null, `${block} through ${[...path_].join(" <- ")}`)];
  },

  constant(read, release, key) {
    const row = Object.values(read("movement-constants.json").constants).find(({ value }) => String(value) === key);
    if (!row) throw new Error(`${key} is no constant of the engine`);
    return [step("generate-movement-constants.cjs", row.files.join(", "), null, row.status === "source" ? `${row.types.join("/")} in the sources of ${row.first} to ${row.last}, ${row.releases} releases` : row.status)];
  },

  shape(read, release, key, root) {
    const [id, channel, index] = key.split("|");
    const report = read("block-shapes.json").releases[release];
    if (report?.status !== "exact") throw new Error(`${release} has no shape table: ${report?.reason}`);
    const table = JSON.parse(zlib.gunzipSync(fs.readFileSync(path.join(root, "block-shapes", `${release}.json.gz`))).toString("utf8"));
    const entry = table.blocks[id];
    if (!entry || entry[channel] === undefined) throw new Error(`${id} ${channel} is not in the table of ${release}`);
    const pick = (value) => (Array.isArray(value) ? value[Number(index)] : value);
    return [step("scripts/java-oracle/ShapeDump.java", `client ${report.client}`, report.remappedSha256 ?? report.client, `${id} state ${index}: ${pick(entry[`${channel}Depends`]) === 0 ? "the state alone" : "contextual"}`), step("generate-block-shapes.cjs", `generated/block-shapes/${release}.json.gz`, report.sha256)];
  },

  friction(read, release, id) {
    const table = read("block-friction.json").releases[release];
    const entry = table.frictions[id] ?? table.default;
    return [step("generate-block-friction.cjs", `${table.source}: ${table.file}`, table.sha256, `${entry.expression}${entry.bits ? ` = ${entry.bits}` : ""}`)];
  },
};

/** One entry's chain. Throws if it does not exist, or if its release's sources have no identity to trace to. */
function explain(kind, release, key, root = path.join(repositoryRoot, "generated")) {
  const chain = CHAINS[kind];
  if (!chain) throw new Error(`Unknown kind ${kind}: ${Object.keys(CHAINS).join(", ")}`);
  const read = reader(root);
  return { kind, release, key, identity: identityOf(read, release), steps: chain(read, release, key, root), behaviour: "not judged here: the recorder's replay against vanilla recordings" };
}

function inventory(root, scripts) {
  const outputs = {};
  for (const [file, generator] of Object.entries(OUTPUTS)) outputs[file] = { sha256: textHash(path.join(root, file)), generator };
  const generators = {};
  for (const generator of [...new Set(Object.values(OUTPUTS)), ...ORACLES].sort()) generators[generator] = textHash(path.join(scripts, generator));
  const tables = {};
  const shapes = path.join(root, "block-shapes");
  for (const file of fs.readdirSync(shapes).sort()) tables[file] = sha256(zlib.gunzipSync(fs.readFileSync(path.join(shapes, file))));
  return { schema: 1, generatedBy: "scripts/provenance.cjs", outputs, generators, shapeTables: tables };
}

/** What differs between a folder and its recorded inventory, each with the file and what changed. */
function staleness(root, scripts = __dirname) {
  const recorded = JSON.parse(fs.readFileSync(path.join(root, "provenance.json"), "utf8"));
  const present = inventory(root, scripts);
  const found = [];
  for (const [file, { sha256: hash }] of Object.entries(present.outputs)) {
    if (recorded.outputs[file]?.sha256 !== hash) found.push(`${file} is not what provenance recorded`);
  }
  for (const [generator, hash] of Object.entries(present.generators)) {
    if (recorded.generators[generator] !== hash) found.push(`${generator} changed since its output was generated`);
  }
  for (const [file, hash] of Object.entries(present.shapeTables)) {
    if (recorded.shapeTables[file] !== hash) found.push(`block-shapes/${file} is not what provenance recorded`);
  }
  for (const file of Object.keys(recorded.shapeTables)) if (!present.shapeTables[file]) found.push(`block-shapes/${file} is gone`);
  return found;
}

function main() {
  const root = process.env.MC_GENERATED_ROOT ?? path.join(repositoryRoot, "generated");
  const [first, release, key] = process.argv.slice(2);
  if (first === "--check") {
    const found = staleness(root, process.env.MC_GENERATOR_ROOT ?? __dirname);
    for (const line of found) console.error(line);
    // (no process.exit after a write to a pipe: on Windows that sometimes never returns, with all of it written)
    if (found.length > 0) process.exitCode = 1;
    return;
  }
  if (first) {
    console.log(JSON.stringify(explain(first, release, key, root), null, 2));
    return;
  }
  fs.writeFileSync(path.join(root, "provenance.json"), `${JSON.stringify(inventory(root, __dirname), null, 1)}\n`);
}

if (require.main === module) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}

module.exports = { explain, staleness, inventory, OUTPUTS, ORACLES };
