// docs/adapter-audit.json: what the adapter hands the engine against what each release's own code answers, channel by channel.
// `actual` is a state whose channel differs on the way to the engine; `masked` is one whose minecraft-data value is wrong and which
// the adapter corrects, so a defect of the data moves nothing; `contextual` counts the answers that need more than the state, held to
// the jars at their contexts by test/contextual-shapes.test.cjs, test/legacy-neighbours.test.cjs and test/legacy-block-entities.test.cjs.
//
// `source` says where the adapter's value came from: `generated` the release's shape table, `external` minecraft-data, `explicit` a
// rule or list of the adapter.
//   node scripts/audit-adapter-world.cjs [--check]
const fs = require("node:fs");
const path = require("node:path");
const zlib = require("node:zlib");
const { registeredReleases, compareReleases } = require("./registered-releases.cjs");
const adapter = require("..");
const native = require("../native/artifacts/native.js");
const { differences } = require("./java-oracle/legacy-neighbours.cjs");
const { compare } = require("./java-oracle/contexts.cjs");
const blockEntities = require("./java-oracle/legacy-block-entities.cjs");

const root = path.resolve(__dirname, "..");
const outputPath = path.join(root, "docs", "adapter-audit.json");
const gunzip = (file) => JSON.parse(zlib.gunzipSync(fs.readFileSync(file)).toString("utf8"));
const at = (values, index) => (Array.isArray(values) ? values[index] : values);
const stateCount = (entry) => entry.properties.reduce((product, [, values]) => product * values.length, 1);
const key = (boxes) => JSON.stringify(boxes ?? []);
const sortedKey = (boxes) => (boxes ?? []).map((box) => box.join()).sort().join(" ");
const solid = ([x0, y0, z0, x1, y1, z1]) => x1 > x0 && y1 > y0 && z1 > z0;
const packageVersion = (name) => require(`${name}/package.json`).version;

const channel = () => ({ compared: 0, actual: 0, masked: 0, contextual: 0, source: { generated: 0, external: 0, explicit: 0 } });

function sentBoxes(updates, field) {
  return updates.flatMap((update) => {
    const box = field === "outline" ? update.outlineShape ?? update.shape : update.interactionShape;
    return box ? [[box.minX, box.minY, box.minZ, box.maxX, box.maxY, box.maxZ]] : [];
  }).filter(solid);
}

function modern(version) {
  const { shapes, blocks } = gunzip(path.join(root, "generated", "block-shapes", `${version}.json.gz`));
  const registry = require("prismarine-registry")(version);
  const Block = require("prismarine-block")(registry);
  const mapper = new adapter.BlockMapper(registry.version, registry, (name, tag) => native.generatedBlockHasTag(version, name, tag),
    (name) => native.generatedMethodShapeBoxes(version, name, "outline"), (name, index, kind) => native.generatedStateShape(version, name, kind, index));
  const channels = { collision: channel(), outline: channel(), interaction: channel(), support: channel(), friction: channel(), fluid: channel() };
  let states = 0;
  for (const [name, entry] of Object.entries(blocks)) {
    for (let index = 0; index < stateCount(entry); index += 1) {
      states += 1;
      const block = Block.fromStateId(entry.firstId + index, 0);
      const described = mapper.describe(block);
      const want = (kind) => shapes[at(entry[kind], index)] ?? null;
      const depends = (kind) => at(entry[`${kind}Depends`], index);
      const updates = mapper.updatesAt((x, y, z) => (x === 0 && y === 0 && z === 0 ? described : null), 0, 0, 0);
      const raw = block.shapes ?? [];
      const tally = (kind, got, rawValue) => {
        const row = channels[kind];
        if (depends(kind) !== 0) {
          row.contextual += 1;
          return;
        }
        row.compared += 1;
        row.source.generated += 1;
        const expected = want(kind) ?? [];
        if (key(got) !== key(expected)) row.actual += 1;
        else if (rawValue !== undefined && sortedKey(rawValue) !== sortedKey(expected)) row.masked += 1;
      };
      tally("collision", mapper.boxesAt((x, y, z) => (x === 0 && y === 0 && z === 0 ? described : null), 0, 0, 0, described), raw);
      if (depends("collision") === 0) tally("outline", sentBoxes(updates, "outline"));
      else channels.outline.contextual += 1;
      tally("interaction", sentBoxes(updates, "interaction"));
      if (depends("collision") === 0 && want("support")) tally("support", mapper.supportShape(described));
      const friction = channels.friction;
      friction.compared += 1;
      friction.source.explicit += 1;
      if (Math.fround(described.friction) !== Math.fround(Number(entry.friction))) friction.actual += 1;
      const fluid = channels.fluid;
      fluid.compared += 1;
      fluid.source.explicit += 1;
      if (Boolean(described.fluid) === at(entry.flags, index).endsWith(":EmptyFluid")) fluid.actual += 1;
    }
  }
  return { states, data: registry.version.minecraftVersion, channels, contexts: compare(version).compared };
}

function legacy(version) {
  const { compared, collision, outline } = differences(version);
  const count = (table) => [...table.values()].reduce((sum, { count: rows }) => sum + rows, 0);
  const entities = blockEntities.compare(version);
  return {
    data: version,
    neighbours: { compared, collisionActual: count(collision), outlineActual: count(outline), outlineBlocks: [...outline.keys()].sort() },
    blockEntities: { compared: entities.compared, actual: count(entities.differ) },
  };
}

function generate() {
  const releases = {};
  for (const { version } of registeredReleases()) {
    releases[version] = compareReleases(version, "1.14") < 0 ? legacy(version) : modern(version);
    process.stderr.write(`${version}\n`);
  }
  return {
    schema: 1,
    generatedBy: "scripts/audit-adapter-world.cjs",
    packages: { "minecraft-data": packageVersion("minecraft-data"), "prismarine-block": packageVersion("prismarine-block"), "prismarine-registry": packageVersion("prismarine-registry") },
    releases,
  };
}

if (require.main === module) {
  const output = `${JSON.stringify(generate(), null, 1)}\n`;
  if (process.argv.includes("--check")) {
    if (fs.readFileSync(outputPath, "utf8").replaceAll("\r\n", "\n") !== output) {
      console.error("docs/adapter-audit.json differs from a fresh audit");
      process.exit(1);
    }
  } else {
    fs.writeFileSync(outputPath, output);
  }
}

module.exports = { generate, modern, legacy };
