// Every block state's geometry as each release's own code gives it (scripts/java-oracle/ShapeDump.java), not as the
// source generators read it: generated/block-shapes/<release>.json.gz, the coverage report generated/block-shapes.json
// and the Rust tables native/data/block_shapes.rs and native/data/shapes/*.rs. All of it is published together, after
// the whole run has passed its checks.
//
// A channel of a state is exact only if the method depends on nothing but the state; what it does depend on is kept
// with it, so a shape that needs the position, the level or an entity cannot be read as static. A release before 1.14
// has none of the methods the oracle runs (no CollisionContext; before 1.13 no shape of a state without a live world):
// the report lists it as unavailable, it does not pass by being left out.
//
// --release V[,V]   only these releases, the others kept
// --dumps DIR       reads DIR/<release>.tsv instead of running java (what they came from is still checked by --check)
// --check           writes nothing, and a difference from the checked-in tables fails
// --cached          a release whose jar is not at hand keeps its table, which is what such a run says it is doing
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const zlib = require("node:zlib");
const { spawnSync } = require("node:child_process");
const { completeClasspath: classpath } = require("./java-oracle/classpath.cjs");
const { clientJarRoot } = require("./source-pins.cjs");
const { openZip } = require("./zip-read.cjs");
const { parseTagFile, TAG_FILE, mergePacks, resolveTags } = require("./generate-block-tags.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const tableRoot = path.join(repositoryRoot, "generated", "block-shapes");
const reportPath = path.join(repositoryRoot, "generated", "block-shapes.json");
const nativeRoot = path.join(repositoryRoot, "native", "data");
const oracleSource = path.join(__dirname, "java-oracle", "ShapeDump.java");
const CHANNELS = ["collision", "outline", "interaction", "support"];
const CHANNEL_KEYS = { c: "collision", o: "outline", i: "interaction", s: "support" };
const ABSENT = "-";
/** The bits of a channel's dependence, in block_shapes.rs too. */
const DEPENDS = { position: 1, level: 2, entity: 4 };
const ABSENT_SHAPE = 0xffff;
const UNAVAILABLE = "the oracle needs CollisionContext, from 1.14; before 1.13 a state has no shape without a live world";

const sha256 = (data) => crypto.createHash("sha256").update(data).digest("hex");
const uniform = (values) => (values.every((value) => value === values[0]) ? values[0] : values);

/** `0.0,0.0,0.0,1.0,1.0,1.0;...` as the numbers: null for a channel the release does not have. */
function parseBoxes(text) {
  if (text === ABSENT) return null;
  if (text === "") return [];
  return text.split(";").map((box) => box.split(",").map(Number));
}

/** channel -> mask of what it depends on, from `c:position+entity,o:level`. */
function parseContext(text) {
  const masks = { collision: 0, outline: 0, interaction: 0, support: 0 };
  if (!text) return masks;
  for (const part of text.split(",")) {
    const [channel, what] = part.split(":");
    if (!CHANNEL_KEYS[channel]) throw new Error(`Unknown channel in context ${text}`);
    for (const name of what.split("+")) {
      if (!DEPENDS[name]) throw new Error(`Unknown dependence in context ${text}`);
      masks[CHANNEL_KEYS[channel]] |= DEPENDS[name];
    }
  }
  return masks;
}

/**
 * A block's properties in the order that numbers its states, each with its values in order: the first property changes
 * slowest. Throws if that does not give every state's own index: a lookup by tuple could not be trusted then.
 */
function stateDomains(name, states) {
  const names = states[0].props === "" ? [] : states[0].props.split(",").map((pair) => pair.split("=")[0]);
  const domains = names.map(() => []);
  const tuples = states.map(({ props }) => (props === "" ? [] : props.split(",").map((pair) => pair.slice(pair.indexOf("=") + 1))));
  for (const tuple of tuples) {
    tuple.forEach((value, at) => {
      if (!domains[at].includes(value)) domains[at].push(value);
    });
  }
  const count = domains.reduce((product, values) => product * values.length, 1);
  if (count !== states.length) throw new Error(`${name}: ${states.length} states are not its ${count} property combinations`);
  tuples.forEach((tuple, index) => {
    if (stateIndex(domains, tuple) !== index) throw new Error(`${name}: state ${index} (${states[index].props}) does not number by its properties`);
  });
  return names.map((property, at) => [property, domains[at]]);
}

function stateIndex(domains, tuple) {
  let index = 0;
  domains.forEach((values, at) => {
    index = index * values.length + values.indexOf(tuple[at]);
  });
  return index;
}

/** A dump as the table: the distinct shapes, and each block's states pointing at them, one value where all agree. */
function parseDump(text, version) {
  const shapes = new Map();
  const shapeId = (boxes) => {
    if (boxes === null) return null;
    const key = JSON.stringify(boxes);
    if (!shapes.has(key)) shapes.set(key, shapes.size);
    return shapes.get(key);
  };
  const blocks = [];
  let tags = null;
  let current = null;
  for (const line of text.split(/\r?\n/)) {
    if (line === "") continue;
    const fields = line.split("\t");
    if (fields[0] === "tags") {
      tags = fields[1];
    } else if (fields[0] === "block") {
      const [, id, className, friction, speedFactor, jumpFactor, defaultIndex] = fields;
      current = { id, className, friction, speedFactor, jumpFactor, default: Number(defaultIndex), states: [] };
      blocks.push(current);
    } else if (fields[0] === "state") {
      const [, id, index, stateId, props, collision, outline, interaction, support, sturdy, flags, context = ""] = fields;
      if (id !== current?.id || Number(index) !== current.states.length) throw new Error(`${version}: ${id} state ${index} is out of order`);
      current.states.push({
        stateId: Number(stateId), props, sturdy, flags,
        collision: shapeId(parseBoxes(collision)),
        outline: shapeId(parseBoxes(outline)),
        interaction: shapeId(parseBoxes(interaction)),
        support: shapeId(parseBoxes(support)),
        depends: parseContext(context),
      });
    } else {
      throw new Error(`${version}: unknown dump line ${fields[0]}`);
    }
  }
  if (!tags) throw new Error(`${version}: the dump does not say how tags were bound`);
  return { tags, blocks, shapes: [...shapes.keys()].map((key) => JSON.parse(key)) };
}

function buildTable(version, dump, identity) {
  const blocks = {};
  for (const block of dump.blocks) {
    const { states } = block;
    const entry = {
      class: block.className.replace(/^net\.minecraft\.world\.level\.block\./, ""),
      friction: block.friction,
      speedFactor: block.speedFactor,
      jumpFactor: block.jumpFactor,
      default: block.default,
      firstId: states[0].stateId,
      properties: stateDomains(block.id, states),
    };
    states.forEach(({ stateId }, index) => {
      if (stateId !== states[0].stateId + index) throw new Error(`${version}: ${block.id}'s state ids are not contiguous`);
    });
    for (const channel of CHANNELS) {
      entry[channel] = uniform(states.map((state) => state[channel]));
      entry[`${channel}Depends`] = uniform(states.map((state) => state.depends[channel]));
    }
    entry.sturdy = uniform(states.map((state) => state.sturdy));
    entry.flags = uniform(states.map((state) => state.flags));
    blocks[block.id] = entry;
  }
  return { schema: 1, release: version, ...identity, tags: dump.tags, shapes: dump.shapes, blocks };
}

const depended = (entry, channel, index) => {
  const depends = entry[`${channel}Depends`];
  return Array.isArray(depends) ? depends[index] : depends;
};

/** How many states of each channel are exact, and which blocks are not, for the report. */
function coverage(table) {
  const channels = Object.fromEntries(CHANNELS.map((channel) => [channel, { exact: 0, contextual: 0, absent: 0, blocks: {} }]));
  let states = 0;
  for (const [id, entry] of Object.entries(table.blocks)) {
    const count = entry.properties.reduce((product, [, values]) => product * values.length, 1);
    states += count;
    for (const channel of CHANNELS) {
      for (let index = 0; index < count; index += 1) {
        const shape = Array.isArray(entry[channel]) ? entry[channel][index] : entry[channel];
        const mask = depended(entry, channel, index);
        const row = channels[channel];
        if (shape === null) row.absent += 1;
        else if (mask === 0) row.exact += 1;
        else {
          row.contextual += 1;
          const names = Object.keys(DEPENDS).filter((name) => mask & DEPENDS[name]).join("+");
          (row.blocks[names] ??= new Set()).add(id);
        }
      }
    }
  }
  for (const row of Object.values(channels)) {
    row.blocks = Object.fromEntries(Object.entries(row.blocks).sort().map(([names, ids]) => [names, [...ids].sort()]));
  }
  return { blocks: Object.keys(table.blocks).length, states, shapes: table.shapes.length, channels };
}

const rustNumber = (value) => (Number.isInteger(value) ? `${value}.0` : String(value));
const moduleName = (version) => `v${version.replaceAll(".", "_")}`;

/** The Rust module of a table: uniform channels are one element, indices shared where the same list recurs. */
function rustModule(table) {
  const lines = ["// @generated by scripts/generate-block-shapes.cjs; do not edit.", "use super::StateShapes;", ""];
  lines.push(`pub static SHAPES: &[&[[f64; 6]]] = &[`);
  for (const boxes of table.shapes) lines.push(`    &[${boxes.map((box) => `[${box.map(rustNumber).join(", ")}]`).join(", ")}],`);
  lines.push("];");
  const lists = new Map();
  const list = (kind, values) => {
    const key = `${kind}:${values.join(",")}`;
    if (!lists.has(key)) {
      const name = `L${lists.size}`;
      lists.set(key, name);
      lines.push(`static ${name}: &[${kind}] = &[${values.join(", ")}];`);
    }
    return lists.get(key);
  };
  const rows = [];
  for (const id of Object.keys(table.blocks).sort()) {
    const entry = table.blocks[id];
    const channel = (name) => {
      const values = Array.isArray(entry[name]) ? entry[name] : [entry[name]];
      return list("u16", values.map((value) => (value === null ? ABSENT_SHAPE : value)));
    };
    const depends = (name) => {
      const values = entry[`${name}Depends`];
      return list("u8", Array.isArray(values) ? values : [values]);
    };
    const domains = entry.properties.map(([name, values]) => `("${name}", &[${values.map((value) => `"${value}"`).join(", ")}])`).join(", ");
    rows.push(`    StateShapes { id: "${id}", properties: &[${domains}], collision: ${channel("collision")}, outline: ${channel("outline")}, interaction: ${channel("interaction")}, support: ${channel("support")}, collision_depends: ${depends("collision")}, outline_depends: ${depends("outline")}, interaction_depends: ${depends("interaction")}, support_depends: ${depends("support")} },`);
  }
  lines.push("pub static BLOCKS: &[StateShapes] = &[", ...rows, "];", "");
  return lines.join("\n");
}

/** `modules`: the module of each release, a release whose table is the same to the byte sharing the first one's. */
function rustIndex(modules) {
  const lines = [...new Set(modules.values())].flatMap((module) => [`#[path = "shapes/${module}.rs"]`, `mod ${module};`]);
  const arms = [...modules].map(([version, module]) => `        "${version}" => Some((${module}::SHAPES, ${module}::BLOCKS)),`);
  return [
    "// @generated by scripts/generate-block-shapes.cjs; do not edit.",
    "// Every state's geometry from each release's own code. A block's channels hold one value for all its states, or one",
    "// per state; a dependence mask other than 0 means the shape is not the state's alone.",
    "#![allow(dead_code)]",
    "",
    "pub const DEPENDS_POSITION: u8 = 1;",
    "pub const DEPENDS_LEVEL: u8 = 2;",
    "pub const DEPENDS_ENTITY: u8 = 4;",
    `pub const ABSENT_SHAPE: u16 = ${ABSENT_SHAPE};`,
    "",
    "pub struct StateShapes {",
    "    pub id: &'static str,",
    "    pub properties: &'static [(&'static str, &'static [&'static str])],",
    ...CHANNELS.map((channel) => `    pub ${channel}: &'static [u16],`),
    ...CHANNELS.map((channel) => `    pub ${channel}_depends: &'static [u8],`),
    "}",
    "",
    ...lines,
    "",
    "/// The shapes and the blocks, sorted by id, of a release with a table.",
    "pub fn table(version: &str) -> Option<(&'static [&'static [[f64; 6]]], &'static [StateShapes])> {",
    "    match version {",
    ...arms,
    "        _ => None,",
    "    }",
    "}",
    "",
    "/// What a release's own code gives for a channel of a state.",
    "#[derive(Clone, Copy, Debug, PartialEq)]",
    "pub enum StateShape {",
    "    /// The state alone decides it.",
    "    Exact(&'static [[f64; 6]]),",
    "    /// It also depends on what the mask says, so no box here is the answer.",
    "    Contextual(u8),",
    "    /// The release has no such method.",
    "    Absent,",
    "}",
    "",
    "impl StateShapes {",
    "    /// The state's index, from a value for each property and nothing else.",
    "    pub fn state_index(&self, values: &[(&str, &str)]) -> Option<usize> {",
    "        if values.len() != self.properties.len() {",
    "            return None;",
    "        }",
    "        let mut index = 0;",
    "        for (name, domain) in self.properties {",
    "            let (_, value) = values.iter().find(|(candidate, _)| candidate == name)?;",
    "            index = index * domain.len() + domain.iter().position(|candidate| candidate == value)?;",
    "        }",
    "        Some(index)",
    "    }",
    "",
    "    pub fn state_count(&self) -> usize {",
    "        self.properties.iter().map(|(_, domain)| domain.len()).product()",
    "    }",
    "",
    "    pub fn shape(&self, shapes: &'static [&'static [[f64; 6]]], kind: &str, index: usize) -> Option<StateShape> {",
    "        if index >= self.state_count() {",
    "            return None;",
    "        }",
    "        let (ids, depends) = match kind {",
    ...CHANNELS.map((channel) => `            "${channel}" => (self.${channel}, self.${channel}_depends),`),
    "            _ => return None,",
    "        };",
    "        let id = *ids.get(index).or(ids.first())?;",
    "        let mask = *depends.get(index).or(depends.first())?;",
    "        Some(if id == ABSENT_SHAPE {",
    "            StateShape::Absent",
    "        } else if mask != 0 {",
    "            StateShape::Contextual(mask)",
    "        } else {",
    "            StateShape::Exact(shapes[usize::from(id)])",
    "        })",
    "    }",
    "}",
    "",
    "pub fn block(version: &str, id: &str) -> Option<(&'static [&'static [[f64; 6]]], &'static StateShapes)> {",
    "    let (shapes, blocks) = table(version)?;",
    "    let at = blocks.binary_search_by(|block| block.id.cmp(id)).ok()?;",
    "    Some((shapes, &blocks[at]))",
    "}",
    "",
  ].join("\n");
}

/** Gzip whose bytes do not say which system made them. */
function compressed(text) {
  const bytes = zlib.gzipSync(text, { level: 9 });
  bytes[9] = 0xff;
  return bytes;
}

function tagsFile(version, directory) {
  const zip = openZip(path.join(clientJarRoot(), version, `minecraft-${version}-client.jar`));
  const base = new Map();
  for (const name of zip.names) {
    const match = TAG_FILE.exec(name);
    if (match && !match[1]) base.set(match[2], parseTagFile(zip.read(name).toString("utf8"), name));
  }
  const resolved = resolveTags(mergePacks([base]), null, version);
  const file = path.join(directory, `${version}.tags`);
  fs.writeFileSync(file, `${[...resolved].map(([id, members]) => `${id}\t${[...members].join(",")}`).join("\n")}\n`);
  return file;
}

/** The java a release needs: JAVA_<major>_HOME, else JAVA_HOME, else the PATH's. */
function javaOf(major) {
  const configured = process.env[`JAVA_${major}_HOME`] ?? process.env.JAVA_HOME;
  if (configured) return path.join(configured, "bin", "java");
  return "java";
}

/** The release's own answers, the oracle compiled once into `classes`. */
function runOracle(version, classes, directory) {
  const { jars, client, remapped, java } = classpath(version);
  const output = path.join(directory, `${version}.tsv`);
  const major = Math.max(java, 21);
  const run = spawnSync(javaOf(major), ["-Xmx1500m", "-cp", [classes, ...jars].join(path.delimiter), "ShapeDump", output, tagsFile(version, directory)], { encoding: "utf8", cwd: directory });
  if (run.status !== 0) throw new Error(`The oracle failed on ${version}: ${(run.stderr ?? run.error?.message ?? "").slice(0, 2000)}`);
  return { text: fs.readFileSync(output, "utf8"), client, remapped };
}

function jarAtHand(version) {
  try {
    classpath(version);
    return true;
  } catch {
    return false;
  }
}

function jarIdentity(version) {
  const { jars, client, remapped } = classpath(version);
  return { client, ...(remapped ? { remappedSha256: sha256(fs.readFileSync(jars[0])) } : {}) };
}

function generate({ only, dumps, cached }) {
  const generated = (name) => JSON.parse(fs.readFileSync(path.join(repositoryRoot, "generated", name), "utf8"));
  const runtime = generated("runtime-registries.json").releases;
  const manifest = generated("release-manifest.json").releases;
  const registered = Array.isArray(manifest) ? manifest.map((entry) => entry.release) : Object.keys(manifest);
  const previous = fs.existsSync(reportPath) ? JSON.parse(fs.readFileSync(reportPath, "utf8")).releases : {};
  const work = fs.mkdtempSync(path.join(os.tmpdir(), "mc-shapes-"));
  try {
    let classes = null;
    if (!dumps) {
      classes = path.join(work, "classes");
      const compile = spawnSync("javac", ["-nowarn", "--release", "21", "-d", classes, oracleSource], { encoding: "utf8" });
      if (compile.status !== 0) throw new Error(`The oracle does not compile: ${compile.stderr}`);
    }
    const tables = new Map();
    const releases = {};
    for (const version of registered) {
      if (!runtime[version]) {
        releases[version] = { status: "unavailable", reason: UNAVAILABLE };
        continue;
      }
      if ((only && !only.includes(version)) || (cached && !jarAtHand(version))) {
        if (!previous[version] || previous[version].status !== "exact") throw new Error(`${version} has no table to keep: generate it too`);
        releases[version] = previous[version];
        tables.set(version, JSON.parse(zlib.gunzipSync(fs.readFileSync(path.join(tableRoot, `${version}.json.gz`))).toString("utf8")));
        continue;
      }
      const { text } = dumps ? { text: fs.readFileSync(path.join(dumps, `${version}.tsv`), "utf8") } : runOracle(version, classes, work);
      const table = buildTable(version, parseDump(text, version), jarIdentity(version));
      const differences = againstClient(table, runtime[version].blocks);
      if (differences.length > 0) throw new Error(`${version} against the client's dump: ${differences.join("; ")}`);
      tables.set(version, table);
      const { client, remappedSha256, tags } = table;
      releases[version] = { status: "exact", client, ...(remappedSha256 ? { remappedSha256 } : {}), tags, sha256: sha256(JSON.stringify(table)), ...coverage(table) };
    }
    return { report: { schema: 1, generatedBy: "scripts/generate-block-shapes.cjs", releases }, tables };
  } finally {
    fs.rmSync(work, { recursive: true, force: true });
  }
}

/**
 * What a table lacks or adds against the running client's own list of [block, state count]: the two come from
 * different runs, and a registration, an inherited property or an alias lost by one of them shows here.
 */
function againstClient(table, clientBlocks) {
  const expected = new Map(clientBlocks);
  const differences = [];
  for (const id of expected.keys()) if (!table.blocks[id]) differences.push(`missing ${id}`);
  for (const [id, entry] of Object.entries(table.blocks)) {
    if (!expected.has(id)) {
      differences.push(`extra ${id}`);
      continue;
    }
    const count = entry.properties.reduce((product, [, values]) => product * values.length, 1);
    if (count !== expected.get(id)) differences.push(`${id} has ${count} states, the client ${expected.get(id)}`);
  }
  return differences;
}

/**
 * Where minecraft-data's collision table, the adapter's input, differs from a table: by state, box order counting. A
 * state whose collision is not static is left out: minecraft-data has its answer at one place in an empty level.
 */
function againstCollisionTable(table, data) {
  const differences = [];
  const boxesOf = (entry, index) => {
    const id = Array.isArray(entry.collision) ? entry.collision[index] : entry.collision;
    return table.shapes[id];
  };
  for (const [id, entry] of Object.entries(table.blocks)) {
    const count = entry.properties.reduce((product, [, values]) => product * values.length, 1);
    const shapes = data.blocks[id];
    if (shapes === undefined) {
      differences.push(`${id} is not in the data`);
      continue;
    }
    for (let index = 0; index < count; index += 1) {
      if (depended(entry, "collision", index) !== 0) continue;
      const theirs = data.shapes[Array.isArray(shapes) ? shapes[index] : shapes];
      if (JSON.stringify(theirs) !== JSON.stringify(boxesOf(entry, index))) differences.push(`${id} state ${index}`);
    }
  }
  return differences;
}

/** Everything a run produces, path -> content. */
function outputs({ report, tables }) {
  const files = new Map([[reportPath, `${JSON.stringify(report, null, 1)}\n`]]);
  // Rust tables the same to the byte are one module: 23 of the 44 are, and each release keeps its own .json.gz and report row
  const modules = new Map();
  const moduleOfText = new Map();
  for (const [version, table] of tables) {
    files.set(path.join(tableRoot, `${version}.json.gz`), compressed(JSON.stringify(table)));
    const text = rustModule(table);
    if (!moduleOfText.has(text)) {
      moduleOfText.set(text, moduleName(version));
      files.set(path.join(nativeRoot, "shapes", `${moduleName(version)}.rs`), text);
    }
    modules.set(version, moduleOfText.get(text));
  }
  files.set(path.join(nativeRoot, "block_shapes.rs"), rustIndex(modules));
  return files;
}

const comparable = (file, content) => (file.endsWith(".gz") ? zlib.gunzipSync(content).toString("latin1") : content.toString("latin1").replaceAll("\r\n", "\n"));

function main() {
  const argument = (name) => {
    const at = process.argv.indexOf(name);
    return at < 0 ? null : process.argv[at + 1];
  };
  const files = outputs(generate({ only: argument("--release")?.split(","), dumps: argument("--dumps"), cached: process.argv.includes("--cached") }));
  if (process.argv.includes("--check")) {
    const drift = [...files].filter(([file, content]) => !fs.existsSync(file)
      || comparable(file, fs.readFileSync(file)) !== comparable(file, Buffer.from(content))).map(([file]) => path.relative(repositoryRoot, file));
    if (drift.length > 0) throw new Error(`Differs from a fresh generation: ${drift.join(", ")}`);
    return;
  }
  const staged = [...files].map(([file, content]) => {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(`${file}.tmp`, content);
    return file;
  });
  for (const file of staged) fs.renameSync(`${file}.tmp`, file);
}

if (require.main === module) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}

module.exports = {
  parseDump, buildTable, stateDomains, stateIndex, parseBoxes, parseContext, coverage, rustModule, againstClient,
  againstCollisionTable, DEPENDS, CHANNELS,
};
