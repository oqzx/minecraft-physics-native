// generated/block-shapes-legacy/<release>.json.gz and generated/block-shapes-legacy.json: each state of the releases before
// 1.14, from their own code in a level of their own (scripts/java-oracle/LegacyShapeDump.java). A block is alone in that
// level, so this says what a state is by itself: what neighbours do to it is for the recordings. A query the blank level
// cannot answer stays as `refused`, with why, and nothing else of the block is taken for it.
//
// It writes src/mineflayer/legacy-full-cubes.json too: by release before 1.13, each block's metadata whose state alone World.isBlockFullCube
// answers true for (its getCollisionBoundingBox's average edge at least 1, 1.12.2 World.java:1835-1838), which the mapper
// sends as the engine's LEGACY_FULL_CUBE tag; the blocks whose box reads their neighbours are the mapper's to answer.
// The jars are the recorder's, Mojang's names of 1.14.4 mapped on by Calamus ids. A library or jar missing stops the run.
//   node scripts/generate-legacy-shapes.cjs [--dumps DIR] [--check]
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const zlib = require("node:zlib");
const { spawnSync } = require("node:child_process");
const { completeClasspath } = require("./java-oracle/classpath.cjs");
const { LEGACY_RELEASES } = require("./legacy-release-data.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const tableRoot = path.join(repositoryRoot, "generated", "block-shapes-legacy");
const reportPath = path.join(repositoryRoot, "generated", "block-shapes-legacy.json");
const oracleSource = path.join(__dirname, "java-oracle", "LegacyShapeDump.java");
const sha256 = (data) => crypto.createHash("sha256").update(data).digest("hex");

/** The release's pinned Mojang-named jar and every library its version json lists (scripts/java-oracle/classpath.cjs). */
const jarsOf = (version) => completeClasspath(version).jars;

const parseBoxes = (text) => (text === "-" ? null : text === "" ? [] : text.split(";").map((box) => box.split(",").map(Number)));

/** A dump as the table: the shapes once, and each state's by its id and properties. */
function parseDump(text, version) {
  const shapes = new Map();
  const shapeId = (boxes) => {
    if (boxes === null) return null;
    const key = JSON.stringify(boxes);
    if (!shapes.has(key)) shapes.set(key, shapes.size);
    return shapes.get(key);
  };
  const blocks = {};
  const refused = {};
  for (const line of text.split(/\r?\n/)) {
    if (line === "") continue;
    const fields = line.split("\t");
    if (fields[0] === "state") {
      const [, name, id, props, collision, outline, full] = fields;
      // (World.isBlockFullCube of the state alone before 1.13, 1 or 0; none from 1.13)
      const flag = full === "1" ? 1 : full === "0" ? 0 : null;
      (blocks[name] ??= []).push([Number(id), props, shapeId(parseBoxes(collision)), shapeId(parseBoxes(outline)), flag]);
    } else if (fields[0] === "refused") {
      const [, name, , , channel, reason] = fields;
      ((refused[name] ??= {})[channel] ??= {})[reason] = ((refused[name][channel] ?? {})[reason] ?? 0) + 1;
    } else {
      throw new Error(`${version}: unknown dump line ${fields[0]}`);
    }
  }
  if (Object.keys(blocks).length < 100) throw new Error(`${version}: the dump has only ${Object.keys(blocks).length} blocks`);
  return { schema: 1, release: version, shapes: [...shapes.keys()].map((key) => JSON.parse(key)), blocks, refused };
}

function dumpOf(version, classes, directory) {
  const output = path.join(directory, `${version}.tsv`);
  const java = process.env.JAVA_21_HOME ? path.join(process.env.JAVA_21_HOME, "bin", "java") : "java";
  const run = spawnSync(java, ["-Xmx1500m", "-XX:-OmitStackTraceInFastThrow", "-XX:+UnlockDiagnosticVMOptions", "-XX:-BytecodeVerificationRemote",
    "-cp", [classes, ...jarsOf(version)].join(path.delimiter), "LegacyShapeDump", output], { encoding: "utf8", cwd: directory });
  if (run.status !== 0) throw new Error(`The oracle failed on ${version}: ${(run.stderr ?? run.error?.message ?? "").slice(0, 2000)}`);
  return fs.readFileSync(output, "utf8");
}

const compressed = (text) => {
  const bytes = zlib.gzipSync(text, { level: 9 });
  bytes[9] = 0xff;
  return bytes;
};

function generate(dumps) {
  const work = fs.mkdtempSync(path.join(os.tmpdir(), "mc-legacy-shapes-"));
  try {
    let classes = null;
    if (!dumps) {
      classes = path.join(work, "classes");
      const compile = spawnSync("javac", ["-nowarn", "--release", "21", "-d", classes, oracleSource], { encoding: "utf8" });
      if (compile.status !== 0) throw new Error(`The oracle does not compile: ${compile.stderr}`);
    }
    const tables = new Map();
    const releases = {};
    for (const { version } of LEGACY_RELEASES) {
      const text = dumps ? fs.readFileSync(path.join(dumps, `${version}.tsv`), "utf8") : dumpOf(version, classes, work);
      const table = parseDump(text, version);
      tables.set(version, table);
      releases[version] = {
        status: "alone",
        states: Object.values(table.blocks).reduce((sum, rows) => sum + rows.length, 0),
        blocks: Object.keys(table.blocks).length,
        refused: Object.fromEntries(Object.entries(table.refused).sort(([a], [b]) => a.localeCompare(b)).map(([name, channels]) => [name, Object.keys(channels)])),
        sha256: sha256(JSON.stringify(table)),
      };
    }
    return { tables, report: { schema: 1, generatedBy: "scripts/generate-legacy-shapes.cjs", releases } };
  } finally {
    fs.rmSync(work, { recursive: true, force: true });
  }
}

function main() {
  const at = process.argv.indexOf("--dumps");
  const { tables, report } = generate(at < 0 ? null : process.argv[at + 1]);
  const text = `${JSON.stringify(report, null, 1)}\n`;
  if (process.argv.includes("--check")) {
    const present = fs.existsSync(reportPath) ? fs.readFileSync(reportPath, "utf8").replaceAll("\r\n", "\n") : "";
    if (present !== text) throw new Error("generated/block-shapes-legacy.json differs from a fresh generation");
    return;
  }
  fs.mkdirSync(tableRoot, { recursive: true });
  for (const [version, table] of tables) fs.writeFileSync(path.join(tableRoot, `${version}.json.gz`), compressed(JSON.stringify(table)));
  const fullCubes = {};
  for (const [version, table] of tables) {
    const byBlock = {};
    for (const [name, rows] of Object.entries(table.blocks)) {
      const metas = rows.filter((row) => row[4] === 1).map(([id]) => id >> 12).sort((a, b) => a - b);
      if (metas.length > 0) byBlock[name] = [...new Set(metas)];
    }
    if (Object.keys(byBlock).length > 0) fullCubes[version] = byBlock;
  }
  fs.writeFileSync(path.join(repositoryRoot, "src", "mineflayer", "legacy-full-cubes.json"), `${JSON.stringify(fullCubes)}
`);
  fs.writeFileSync(reportPath, text);
}

if (require.main === module) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}

module.exports = { parseDump };
