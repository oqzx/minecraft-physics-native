// generated/entity-data.json: what each release's own code says its entities measure and its attributes and effects are
// (scripts/java-oracle/EntityDump.java). An eye height is a value here from 1.20.5; before, it is a method of a live
// entity, so scripts/java-oracle/LiveEntityDump.java builds the entities and asks them: `live`, with `noLive` naming
// those a blank level cannot build and why, and `playerEyes` for the player, whose type has no factory. A release before
// 1.14 has only what scripts/java-oracle/LegacyEntityDump.java reads from a built entity: status "legacy", its `live`
// with each entity's `size`. A release whose jar is not at hand is listed as unavailable.
//   node scripts/generate-entity-data.cjs [--dumps DIR] [--check] [--cached] [--only 26.2,26.3]
// With --cached a release whose jar is not at hand keeps its entry, which is what such a run says it is doing.
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const { completeClasspath: classpath } = require("./java-oracle/classpath.cjs");
const { clientJarRoot } = require("./source-pins.cjs");
const { openZip } = require("./zip-read.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.join(repositoryRoot, "generated", "entity-data.json");
const oracleSources = ["EntityDump", "LiveEntityDump", "LegacyEntityDump"].map((name) => path.join(__dirname, "java-oracle", `${name}.java`));
const ABSENT = "-";
const number = (text) => (text === ABSENT ? null : Number(text));
const UNAVAILABLE = "no jar of this release with the recorder's names is at hand";
/** Before 1.14 a mapped class can sit in another package than the Mojang class it reads: see OracleAttributesLegacy. */
const LEGACY_JVM = ["-XX:+UnlockDiagnosticVMOptions", "-XX:-BytecodeVerificationRemote"];

function dimensions(fields) {
  const [width, height, fixed, eyeHeight] = fields;
  return { width: number(width), height: number(height), fixed: fixed === "true", eyeHeight: number(eyeHeight) };
}

function parseDump(text, version) {
  const release = { entities: {}, poses: {}, attributes: {}, effects: {}, callbacks: {}, enchantments: {} };
  for (const line of text.split(/\r?\n/)) {
    if (line === "") continue;
    const [kind, id, ...fields] = line.split("\t");
    if (kind === "entity") release.entities[id] = { category: fields[0], ...dimensions(fields.slice(1)) };
    else if (kind === "pose") release.poses[id] = dimensions(fields);
    else if (kind === "attribute") {
      const [value, min, max, syncable] = fields;
      release.attributes[id] = { default: number(value), min: number(min), max: number(max), syncable: syncable === "true" };
    } else if (kind === "enchantment") {
      const [maxLevel, slots] = fields;
      release.enchantments[id] = { maxLevel: number(maxLevel), slots: slots ? slots.split(",") : [], attributes: [] };
    } else if (kind === "callback") {
      const [method, owner] = fields;
      (release.callbacks[method] ??= {})[id] = owner;
    } else if (kind === "effect") {
      const [category, ...modifiers] = fields;
      release.effects[id] = {
        category,
        modifiers: modifiers.map((modifier) => {
          const [attribute, amount, operation] = modifier.split("|");
          if (amount === ABSENT || operation === ABSENT) throw new Error(`${version}: ${id}'s modifier of ${attribute} was not read`);
          return { attribute, amount: Number(amount), operation };
        }),
      };
    } else {
      throw new Error(`${version}: unknown dump line ${kind}`);
    }
  }
  for (const part of ["entities", "poses", "attributes", "effects", "callbacks"]) {
    if (Object.keys(release[part]).length === 0) throw new Error(`${version}: the dump has no ${part}`);
  }
  return release;
}

/** The enchantments of a release's data pack, from 1.21: their levels, slots and what each does to an attribute. */
function dataEnchantments(version) {
  const zip = openZip(path.join(clientJarRoot(), version, `minecraft-${version}-client.jar`));
  const found = {};
  for (const name of zip.names) {
    const match = /^data\/minecraft\/enchantment\/([a-z_]+)\.json$/.exec(name);
    if (!match) continue;
    const parsed = JSON.parse(zip.read(name).toString("utf8"));
    found[match[1]] = {
      maxLevel: parsed.max_level,
      slots: [...parsed.slots].sort(),
      attributes: (parsed.effects?.["minecraft:attributes"] ?? []).map(({ attribute, operation, amount }) => ({
        attribute: attribute.replace(/^minecraft:/, ""), operation, amount,
      })),
    };
  }
  return found;
}

/** What a built entity answered: entity -> its eyes and seats, and the ones that could not be built. */
function parseLive(text, version) {
  const live = {};
  const noLive = {};
  const playerEyes = {};
  for (const line of text.split(/\r?\n/)) {
    if (line === "") continue;
    const [kind, id, ...fields] = line.split("\t");
    if (kind === "live") {
      const [eye, crouching, seats, riding, maxPassengers] = fields;
      live[id] = { ...live[id], eye: number(eye), eyeCrouching: number(crouching), passengersRidingOffset: number(seats), myRidingOffset: number(riding), maxPassengers: number(maxPassengers) };
    } else if (kind === "nolive") noLive[id] = fields[0];
    else if (kind === "playereye") playerEyes[id] = number(fields[0]);
    else if (kind === "attachment") {
      const [slot, x, y, z] = fields;
      ((live[id] ??= {}).attachments ??= [])[Number(slot)] = [number(x), number(y), number(z)];
    } else if (kind === "riderpoint") (live[id] ??= {}).riderPoint = fields.map(number);
    else if (kind === "riderfn") (live[id] ??= {}).riderComputedBy = fields[0];
    else if (kind === "size") (live[id] ??= {}).size = fields.map(number);
    else if (kind === "ridercase") {
      const [slot, yaw, size, x, y, z] = fields;
      ((live[id] ??= {}).riderCases ??= []).push([Number(slot), Number(yaw), size === "baby" ? size : Number(size), number(x), number(y), number(z)]);
    }
    else throw new Error(`${version}: unknown live line ${kind}`);
  }
  if (Object.keys(live).length === 0) throw new Error(`${version}: no entity could be built`);
  return { live, noLive, playerEyes };
}

/** The java a release needs: JAVA_<major>_HOME, else JAVA_HOME, else the PATH's. */
function javaOf(major) {
  const configured = process.env[`JAVA_${major}_HOME`] ?? process.env.JAVA_HOME;
  if (configured) return path.join(configured, "bin", "java");
  return "java";
}

function dumpOf(version, classes, directory, oracle) {
  const { jars, java } = classpath(version);
  const output = path.join(directory, `${version}.${oracle}.tsv`);
  const flags = oracle === "LegacyEntityDump" ? LEGACY_JVM : [];
  const run = spawnSync(javaOf(Math.max(java, 21)), [...flags, "-Xmx1200m", "-cp", [classes, ...jars].join(path.delimiter), oracle, output], { encoding: "utf8", cwd: directory });
  if (run.status !== 0) throw new Error(`The oracle failed on ${version}: ${(run.stderr ?? run.error?.message ?? "").slice(0, 2000)}`);
  return fs.readFileSync(output, "utf8");
}

function jarAtHand(version) {
  try {
    classpath(version);
    return true;
  } catch {
    return false;
  }
}

function generate(dumps, cached, only = null) {
  const generated = (name) => JSON.parse(fs.readFileSync(path.join(repositoryRoot, "generated", name), "utf8"));
  const manifest = generated("release-manifest.json").releases;
  const versions = Array.isArray(manifest) ? manifest.map((entry) => entry.release) : Object.keys(manifest);
  const runtime = generated("runtime-registries.json").releases;
  const kept = (version) => (only ? !only.includes(version) : cached && !dumps && !jarAtHand(version));
  const previous = (cached || only) && fs.existsSync(outputPath) ? JSON.parse(fs.readFileSync(outputPath, "utf8")).releases : {};
  const work = fs.mkdtempSync(path.join(os.tmpdir(), "mc-entities-"));
  try {
    let classes = null;
    if (!dumps && versions.some((version) => !kept(version))) {
      classes = path.join(work, "classes");
      const compile = spawnSync("javac", ["-nowarn", "--release", "21", "-d", classes, ...oracleSources], { encoding: "utf8" });
      if (compile.status !== 0) throw new Error(`The oracle does not compile: ${compile.stderr}`);
    }
    const releases = {};
    for (const version of versions) {
      if (kept(version)) {
        if (!previous[version]) throw new Error(`${version} has no entry to keep: its jar is not at hand`);
        releases[version] = previous[version];
        continue;
      }
      if (!runtime[version]) {
        const legacyDump = dumps ? path.join(dumps, `${version}.LegacyEntityDump.tsv`) : null;
        // (without --cached a legacy jar not at hand fails the run: a host lacking it must not publish "unavailable")
        if (dumps ? !fs.existsSync(legacyDump) : cached && !jarAtHand(version)) {
          releases[version] = { status: "unavailable", reason: UNAVAILABLE };
          continue;
        }
        const text = dumps ? fs.readFileSync(legacyDump, "utf8") : dumpOf(version, classes, work, "LegacyEntityDump");
        releases[version] = { status: "legacy", client: dumps ? null : classpath(version).client, ...parseLive(text, version) };
        continue;
      }
      const text = dumps ? fs.readFileSync(path.join(dumps, `${version}.EntityDump.tsv`), "utf8") : dumpOf(version, classes, work, "EntityDump");
      const live = dumps ? fs.readFileSync(path.join(dumps, `${version}.LiveEntityDump.tsv`), "utf8") : dumpOf(version, classes, work, "LiveEntityDump");
      const parsed = parseDump(text, version);
      if (Object.keys(parsed.enchantments).length === 0) parsed.enchantments = dataEnchantments(version);
      if (Object.keys(parsed.enchantments).length === 0) throw new Error(`${version}: no enchantments in its code or its data`);
      releases[version] = { status: "exact", client: classpath(version).client, ...parsed, ...parseLive(live, version) };
    }
    return { schema: 1, generatedBy: "scripts/generate-entity-data.cjs", releases };
  } finally {
    fs.rmSync(work, { recursive: true, force: true });
  }
}

function main() {
  const at = process.argv.indexOf("--dumps");
  const onlyAt = process.argv.indexOf("--only");
  const only = onlyAt < 0 ? null : process.argv[onlyAt + 1].split(",");
  const output = `${JSON.stringify(generate(at < 0 ? null : process.argv[at + 1], process.argv.includes("--cached"), only))}\n`;
  if (process.argv.includes("--check")) {
    const present = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, "utf8").replaceAll("\r\n", "\n") : "";
    if (present !== output) throw new Error("generated/entity-data.json differs from a fresh generation");
    return;
  }
  fs.writeFileSync(`${outputPath}.tmp`, output);
  fs.renameSync(`${outputPath}.tmp`, outputPath);
}

if (require.main === module) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}

module.exports = { parseDump, parseLive };
