// src/mineflayer/context-outlines.json: the outline of the blocks whose outline is no state's alone, so generated/block-shapes has
// no boxes for it (status `contextual`), from what the jars answered at several cells and in an entity's context
// (test/fixtures/contexts, scripts/java-oracle/OracleContext.java): each state's box at no offset, and the block's maximum offset.
// A release that answers otherwise than a table already made gets its own. 1.13.2 has no shape table the mapper reads, so its table is
// every outline that is not the state's collision, from generated/block-shapes-legacy (LegacyShapeDump.java).
//   node scripts/generate-context-outlines.cjs
const fs = require("node:fs");
const path = require("node:path");
const zlib = require("node:zlib");
const { horizontalOffset } = require("../dist/mineflayer/blocks.js");

const root = path.resolve(__dirname, "..");
const folder = path.join(root, "test", "fixtures", "contexts");
const output = path.join(root, "src", "mineflayer", "context-outlines.json");
const OFFSETS = [0.25, 0.125];
const CONTEXT_FREE = new Set(["scaffolding", "light"]);
const NEIGHBOURS = /fence|_wall$|pane$|^iron_bars$|^chorus_plant$|chest$/;
const boxesOf = (text) => (text === "" || text === undefined ? [] : text.split(";").map((box) => box.split(",").map(Number)));

/** The block's states at each cell without the offset of a maximum, or null when one maximum does not explain them. */
function atNoOffset(rows, maximum) {
  const states = new Map();
  for (const { index, x, z, boxes } of rows) {
    const [ox, oz] = horizontalOffset(x, z, maximum);
    const base = JSON.stringify(boxes.map(([a, b, c, d, e, f]) => [a - ox, b, c - oz, d - ox, e, f - oz]));
    if (states.has(index) && states.get(index) !== base) return null;
    states.set(index, base);
  }
  return states;
}

function tableOf(lines) {
  const moving = new Map();
  const plain = new Map();
  for (const line of lines) {
    const row = line.split("\t");
    if (row[0] === "position") {
      const [, name, index, x, , z, , outline] = row;
      if (!moving.has(name)) moving.set(name, []);
      moving.get(name).push({ index: Number(index), x: Number(x), z: Number(z), boxes: boxesOf(outline) });
    } else if (row[0] === "entity" && CONTEXT_FREE.has(row[1])) {
      if (!plain.has(row[1])) plain.set(row[1], new Map());
      plain.get(row[1]).set(Number(row[2]), JSON.stringify(boxesOf(row[10])));
    }
  }
  const table = {};
  const entry = (name, maximum, states) => {
    const shapes = [...new Set(states.values())];
    const count = Math.max(...states.keys()) + 1;
    table[name] = { max: maximum, shapes: shapes.map((text) => JSON.parse(text)), states: Array.from({ length: count }, (_, i) => (states.has(i) ? shapes.indexOf(states.get(i)) : -1)) };
  };
  for (const [name, rows] of [...moving].sort()) {
    const found = OFFSETS.map((maximum) => [maximum, atNoOffset(rows, maximum)]).find(([, states]) => states);
    if (!found) throw new Error(`no offset explains ${name}`);
    entry(name, found[0], found[1]);
  }
  for (const [name, states] of [...plain].sort()) entry(name, 0, states);
  return table;
}

const tables = [];
const releases = {};
for (const file of fs.readdirSync(folder).sort()) {
  const version = file.replace(".tsv.gz", "");
  const table = tableOf(zlib.gunzipSync(fs.readFileSync(path.join(folder, file))).toString("utf8").split("\n"));
  const text = JSON.stringify(table);
  let at = tables.findIndex((other) => JSON.stringify(other) === text);
  if (at < 0) at = tables.push(table) - 1;
  releases[version] = at;
}
/**
 * Before 1.13 a ray meets one box where the collision may be others (a cactus, a hopper), by data value: every state with a collision
 * box whose outline is not it. A fence, a pane and a wall are the mapper's own (legacyJoinedOutline), their outline being their neighbours'.
 */
function tableBefore113(version) {
  const lines = zlib.gunzipSync(fs.readFileSync(path.join(root, "test", "fixtures", "legacy-neighbours", `${version}.tsv.gz`))).toString("utf8").split("\n").map((line) => line.split("\t"));
  const shapes = new Map(lines.filter(([kind]) => kind === "shape").map(([, index, text]) => [Number(index), text ?? ""]));
  const table = {};
  for (const [kind, name, id, collision, outline] of lines) {
    if (kind !== "alone" || shapes.get(Number(outline)) === "-" || shapes.get(Number(collision)) === "" || NEIGHBOURS.test(name)) continue;
    if (shapes.get(Number(collision)) === shapes.get(Number(outline))) continue;
    const entry = (table[name] ??= { max: 0, shapes: [], states: [] });
    const text = JSON.stringify(boxesOf(shapes.get(Number(outline))));
    let at = entry.shapes.findIndex((other) => JSON.stringify(other) === text);
    if (at < 0) at = entry.shapes.push(JSON.parse(text)) - 1;
    entry.states[Number(id) >> 12] = at;
  }
  for (const entry of Object.values(table)) entry.states = Array.from(entry.states, (value) => value ?? -1);
  return table;
}

/** 1.13.2: each block whose outline is not its collision, a plant's at no offset by the maximum 1.14.4 holds for it. */
function table113(offsets) {
  const { shapes, blocks } = JSON.parse(zlib.gunzipSync(fs.readFileSync(path.join(root, "generated", "block-shapes-legacy", "1.13.2.json.gz"))).toString("utf8"));
  const table = {};
  for (const [name, rows] of Object.entries(blocks)) {
    const first = Math.min(...rows.map(([id]) => id));
    const maximum = offsets[name]?.max ?? 0;
    const [ox, oz] = horizontalOffset(0, 0, maximum);
    const base = ([a, b, c, d, e, f]) => [a - ox, b, c - oz, d - ox, e, f - oz];
    const states = rows.map(([id, , collision, outline]) => [id - first, outline === null ? [] : shapes[outline].map(base), shapes[collision] ?? []]);
    if (states.every(([, outline, collision]) => JSON.stringify(outline) === JSON.stringify(collision))) continue;
    const list = [...new Set(states.map(([, outline]) => JSON.stringify(outline)))];
    const indexes = [];
    for (const [index, outline] of states) indexes[index] = list.indexOf(JSON.stringify(outline));
    table[name] = { max: maximum, shapes: list.map((text) => JSON.parse(text)), states: indexes };
  }
  return table;
}

releases["1.13.2"] = tables.push(table113(tables[releases["1.14.4"]])) - 1;
for (const version of ["1.8.8", "1.9.4", "1.10.2", "1.11.2", "1.12.2"]) releases[version] = tables.push(tableBefore113(version)) - 1;
fs.writeFileSync(output, `${JSON.stringify({ releases, tables })}\n`);
console.log(`${Object.keys(releases).length} releases, ${tables.length} tables`);
