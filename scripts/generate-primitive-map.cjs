// generated/primitive-map.json: which vanilla declarations each collision primitive of the engine ports, and what those
// call. The table below is reviewed by hand, as porting is. What it claims is checked here: each declaration must exist
// in the release's source, and its fingerprint per release shows where the vanilla method changed, so a change has to be
// looked at. Helpers are read from the source: the methods of the shape classes a declaration calls, followed.
//
// A release before 1.14 has its own classes: LEGACY below maps them on the trees scripts/decompile-legacy-trees.cjs makes, which
// carry the recorder's names (Mojang's where a name crossed over, method_N where none did). Those trees are not pinned inputs, so
// a machine without them keeps the `legacy` section the file already has; MC_LEGACY_TREES names another folder.
//
// `decisions` keeps what equal end positions cannot show: of each method that walks blocks or combines shapes, the branches, returns
// and shape calls in source order, per era; and `simplification`, each place a release calls VoxelShape.optimize.
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { registeredReleases, compareReleases } = require("./registered-releases.cjs");
const { methodsNamed } = require("./java-declarations.cjs");
const { modernTree } = require("./source-roots.cjs");
const { sourceCache } = require("./source-pins.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.join(repositoryRoot, "generated", "primitive-map.json");

const SHAPES = "net/minecraft/world/phys/shapes";
const FILES = {
  Shapes: `${SHAPES}/Shapes.java`,
  VoxelShape: `${SHAPES}/VoxelShape.java`,
  ArrayVoxelShape: `${SHAPES}/ArrayVoxelShape.java`,
  CubeVoxelShape: `${SHAPES}/CubeVoxelShape.java`,
  DiscreteVoxelShape: `${SHAPES}/DiscreteVoxelShape.java`,
  BitSetDiscreteVoxelShape: `${SHAPES}/BitSetDiscreteVoxelShape.java`,
  IndexMerger: `${SHAPES}/IndexMerger.java`,
  IndirectMerger: `${SHAPES}/IndirectMerger.java`,
  Cursor3D: "net/minecraft/core/Cursor3D.java",
  BlockCollisions: "net/minecraft/world/level/BlockCollisions.java",
  CollisionGetter: "net/minecraft/world/level/CollisionGetter.java",
  BlockGetter: "net/minecraft/world/level/BlockGetter.java",
  AABB: "net/minecraft/world/phys/AABB.java",
  Entity: "net/minecraft/world/entity/Entity.java",
  CollisionSpliterator: "net/minecraft/world/level/CollisionSpliterator.java",
  Level: "net/minecraft/world/level/Level.java",
  LevelReader: "net/minecraft/world/level/LevelReader.java",
  PistonMovingBlockEntity: "net/minecraft/world/level/block/piston/PistonMovingBlockEntity.java",
  WorldBorder: "net/minecraft/world/level/border/WorldBorder.java",
};

const legacyTrees = () => process.env.MC_LEGACY_TREES ?? path.join(path.dirname(sourceCache()), "legacy-src");

/** What each native primitive ports on the releases whose classes the modern table does not name. */
const BEFORE_VOXELS = {
  "collision::clip_axis": ["AABB#method_583", "AABB#method_589", "AABB#method_594"],
  "collision::collide_legacy": ["Entity#move", "Level#doesBoxCollide", "AABB#method_583", "AABB#method_589", "AABB#method_594"],
  "collision::from_world": ["Level#doesBoxCollide"],
  "block_traversal::blocks_visited": ["Entity#checkInsideBlocks"],
  "block_traversal::blocks_visited_where": ["Entity#checkInsideBlocks"],
};
const WITH_WALK = { ...BEFORE_VOXELS, "collision::from_world": ["Level#doesBoxCollide", "Level#method_13689"] };
const LEGACY = {
  "1.8.8": BEFORE_VOXELS,
  "1.9.4": BEFORE_VOXELS,
  "1.10.2": BEFORE_VOXELS,
  "1.11.2": WITH_WALK,
  "1.12.2": WITH_WALK,
  "1.13.2": {
    "voxel_shape::empty": ["Shapes#empty"],
    "voxel_shape::from_box": ["Shapes#box", "Shapes#create"],
    "voxel_shape::union": ["Shapes#or", "Shapes#join", "VoxelShape#optimize"],
    "voxel_shape::boxes": ["VoxelShape#toAabbs", "VoxelShape#forAllBoxes"],
    "voxel_shape::is_empty": ["VoxelShape#isEmpty"],
    "voxel_shape::bounds": ["VoxelShape#bounds"],
    "voxel_shape::collide": ["VoxelShape#collide", "VoxelShape#collideX"],
    "voxel_shape::collide_in": ["VoxelShape#collide", "VoxelShape#collideX"],
    "voxel_shape::translated": ["VoxelShape#move"],
    "collision::clip_axis": ["VoxelShape#collide", "VoxelShape#collideX"],
    "collision::collide_legacy": ["Entity#move", "Shapes#collide", "LevelReader#method_16378"],
    "collision::from_world": ["LevelReader#method_16378", "LevelReader#getCollisions"],
    "collision::grid": ["ArrayVoxelShape#getCoords", "CubeVoxelShape#getCoords"],
    "collision::grid_lines": ["ArrayVoxelShape#getCoords", "CubeVoxelShape#getCoords"],
    "collision::collide_over_grid": ["VoxelShape#collideX"],
    "collision::grid_bits": ["Shapes#findBits"],
    "block_traversal::blocks_visited": ["Entity#checkInsideBlocks"],
    "block_traversal::blocks_visited_where": ["Entity#checkInsideBlocks"],
  },
};

/** The methods whose order of decisions is the behaviour: the walks of blocks, the joins and the clips. */
const DECISION_METHODS = [
  "BlockCollisions#computeNext", "CollisionSpliterator#tryAdvance", "CollisionSpliterator#collisionCheck", "Cursor3D#advance", "CollisionGetter#noCollision", "CollisionGetter#getBlockCollisions", "CollisionGetter#noBlockCollision",
  "BlockGetter#traverseBlocks", "BlockGetter#boxTraverseBlocks", "BlockGetter#forEachBlockIntersectedBetween",
  "Shapes#or", "Shapes#join", "Shapes#joinUnoptimized", "Shapes#joinIsNotEmpty", "Shapes#collide", "Shapes#findBits",
  "VoxelShape#collide", "VoxelShape#collideX", "VoxelShape#optimize", "VoxelShape#toAabbs",
  "Entity#collide", "Entity#collideBoundingBox", "Entity#collideWithShapes", "Entity#checkInsideBlocks", "Entity#move",
  "Level#doesBoxCollide", "Level#method_13689", "LevelReader#method_16378", "LevelReader#getCollisions",
];

/** Where a release may simplify a shape at run time; a block's own constants are in generated/block-shapes as what they became. */
const SIMPLIFICATION_SCOPE = ["Shapes", "VoxelShape", "BlockCollisions", "CollisionSpliterator", "CollisionGetter", "BlockGetter", "Entity", "Level", "LevelReader", "PistonMovingBlockEntity", "WorldBorder"];

/** native module::function -> `Class#method`, or `own` with the reason it ports nothing by itself. */
const PRIMITIVES = {
  "voxel_shape::empty": ["Shapes#empty"],
  "voxel_shape::from_box": ["Shapes#box", "Shapes#create"],
  "voxel_shape::union": ["Shapes#or", "Shapes#joinUnoptimized", "VoxelShape#optimize"],
  "voxel_shape::boxes": ["VoxelShape#toAabbs", "VoxelShape#forAllBoxes", "BitSetDiscreteVoxelShape#forAllBoxes"],
  "voxel_shape::is_empty": ["VoxelShape#isEmpty"],
  "voxel_shape::bounds": ["VoxelShape#bounds"],
  "voxel_shape::collide": ["VoxelShape#collide", "VoxelShape#collideX"],
  "voxel_shape::collide_in": ["VoxelShape#collide", "VoxelShape#collideX"],
  "voxel_shape::translated": ["VoxelShape#move"],
  "voxel_shape::rotated_y_quarters": { own: "a test and table builder: vanilla rotates by Shapes.rotate from 1.21.2, data before" },
  "voxel_shape::rotated_x_quarters": { own: "a test and table builder: see rotated_y_quarters" },
  "voxel_shape::rotated_z_quarters": { own: "a test and table builder: see rotated_y_quarters" },
  "collision::vanilla_equal": ["Shapes#collide"],
  "collision::clip_axis": ["VoxelShape#collide", "VoxelShape#collideX"],
  "collision::reaches_into": ["VoxelShape#collideX"],
  "collision::new": { own: "the constructor of the engine's shape cell" },
  "collision::moving_piston": { own: "a setter of the engine's shape cell: whether the block is MOVING_PISTON, which Shapes.collide's walk reads on an edge (1.14.4 Shapes.java:239, 1.17.1 :268)" },
  "collision::from_world": ["BlockCollisions#computeNext", "CollisionSpliterator#tryAdvance", "CollisionSpliterator#collisionCheck", "LevelReader#getCollisions"],
  "collision::grid": ["ArrayVoxelShape#getCoords", "CubeVoxelShape#getCoords"],
  "collision::sort_in_cursor_order": ["BlockCollisions#computeNext", "Cursor3D#advance"],
  "collision::grid_lines": ["ArrayVoxelShape#getCoords", "CubeVoxelShape#getCoords"],
  "collision::collide_over_grid": ["VoxelShape#collideX"],
  "collision::collide_shapes": ["Shapes#collide", "Entity#collideWithShapes"],
  "collision::of": { own: "the constructor of a list element" },
  "collision::grid_bits": ["Shapes#findBits"],
  "collision::collide_legacy": { own: "before 1.13: AxisAlignedBB.calculateXOffset, Yarn's names, outside this table" },
  "block_traversal::blocks_visited": ["BlockGetter#traverseBlocks"],
  "block_traversal::blocks_visited_where": ["BlockGetter#traverseBlocks"],
  "block_traversal::box_traverse_blocks": ["BlockGetter#boxTraverseBlocks"],
  "block_traversal::blocks_intersected_between": ["BlockGetter#forEachBlockIntersectedBetween", "BlockGetter#addCollisionsAlongTravel"],
  "block_traversal::blocks_intersected_between_in_direction": ["BlockGetter#forEachBlockIntersectedBetween", "BlockGetter#addCollisionsAlongTravel"],
};

const sha256 = (text) => crypto.createHash("sha256").update(text).digest("hex");

/** Native's public functions of the modules the table covers. */
function nativePrimitives() {
  const found = [];
  for (const module of ["voxel_shape", "collision", "block_traversal"]) {
    const source = fs.readFileSync(path.join(repositoryRoot, "native", "src", `${module}.rs`), "utf8");
    const code = source.split(/#\[cfg\(test\)\]\s*mod\s/)[0];
    for (const match of code.matchAll(/^\s*pub(?:\(crate\))? fn ([a-z_0-9]+)/gm)) found.push(`${module}::${match[1]}`);
  }
  return [...new Set(found)].sort();
}

function declaredMethods(source) {
  const names = new Set();
  for (const match of source.matchAll(/^\s+(?:public|protected|private|static|final|abstract|default|synchronized|\s)*[\w<>[\], .?]+\s+([a-zA-Z_]\w*)\s*\([^;]*\)\s*(?:throws [\w, .]+)?\s*\{/gm)) names.add(match[1]);
  return names;
}

/** The shape classes' methods a method calls, followed to the end, `Class#method`. */
function helperChain(start, sources) {
  const seen = new Set();
  const visit = (reference) => {
    if (seen.has(reference)) return;
    const [owner, name] = reference.split("#");
    const source = sources.get(owner);
    // (a move's own class is no helper of the shapes: followed, Entity would bring the whole entity in)
    if (!source || (owner === "Entity" && reference !== start)) return;
    const declarations = declared(source, name);
    if (declarations.length === 0) return;
    seen.add(reference);
    for (const { source: body } of declarations) {
      for (const call of body.matchAll(/(?:\b([A-Z]\w*)\.)?\b([a-z]\w*)\(/g)) {
        const target = call[1] && sources.has(call[1]) ? call[1] : owner;
        if (call[2] !== name && sources.get(target)?.declared.has(call[2])) visit(`${target}#${call[2]}`);
      }
    }
  };
  visit(start);
  seen.delete(start);
  return [...seen].sort();
}

/**
 * A method's control flow and shape calls in source order: a branch or loop with its condition, a return, a jump, and every call of
 * a shape operation, a BooleanOp and a tolerance, local names erased so that a decompiler's naming is no change.
 */
function decisionsOf(body) {
  const found = [];
  const start = body.indexOf("{");
  const code = body.slice(start < 0 ? 0 : start);
  const erase = (text) => text.replace(/\s+/g, " ").replace(/(?<![.\w])[a-z]\w*\b(?!\s*\()/g, (word) => (/^(this|new|null|true|false|return|instanceof|double|float|int|long|boolean|final)$/.test(word) ? word : "_")).trim();
  const balanced = (from) => {
    let depth = 0;
    for (let at = from; at < code.length; at += 1) {
      if (code[at] === "(") depth += 1;
      else if (code[at] === ")" && --depth === 0) return at + 1;
    }
    return code.length;
  };
  for (const match of code.matchAll(/\b(if|while|for|switch)\s*\(|\b(return)\b[^;]*;|\b(continue|break)\b\s*;|\b(?:Shapes|BooleanOp)\.\w+|\.optimize\(\)|\b\d[\d.]*E-?\d+[FfDd]?/g)) {
    if (match[1]) {
      const open = match.index + match[0].length - 1;
      found.push([match.index, `${match[1]} ${erase(code.slice(open, balanced(open)))}`]);
    } else if (match[2]) found.push([match.index, erase(match[0])]);
    else if (match[3]) found.push([match.index, match[3]]);
    else found.push([match.index, match[0]]);
  }
  return found.sort((a, b) => a[0] - b[0]).map(([, text]) => text);
}

/** Each `Class#method` of the scope holding a call of optimize. */
function simplificationSites(sources) {
  const sites = [];
  for (const owner of SIMPLIFICATION_SCOPE) {
    const source = sources.get(owner);
    if (!source || !String(source).includes(".optimize()")) continue;
    for (const name of source.declared) {
      if (declared(source, name).some(({ source: body }) => /\.optimize\(\)/.test(body))) sites.push(`${owner}#${name}`);
    }
  }
  return sites.sort();
}

function keepDecisions(decisions, sites, version, sources) {
  for (const reference of DECISION_METHODS) {
    const [owner, name] = reference.split("#");
    if (sources.empty.includes(owner)) continue;
    const source = sources.get(owner);
    const found = source ? declared(source, name) : [];
    if (found.length === 0) continue;
    const list = found.map(({ source: body }) => decisionsOf(body));
    const text = JSON.stringify(list);
    const row = (decisions[reference] ??= []);
    if (row.at(-1)?.text !== text) row.push({ from: version, text, list });
  }
  if (sources.empty.length > 0) return;
  const text = JSON.stringify(simplificationSites(sources));
  if (sites.at(-1)?.text !== text) sites.push({ from: version, text, list: JSON.parse(text) });
}

const parsed = new Map();
/** methodsNamed once for a text: most releases share these files. */
function declared(source, name) {
  const text = String(source);
  const key = `${sha256(text)}#${name}`;
  if (!parsed.has(key)) parsed.set(key, methodsNamed(text, name));
  return parsed.get(key);
}

/** A file the tree holds empty (1.18.1's Shapes.java, which its decompile lost) says nothing: its release is `unread`, no era of its own. */
function readSources(tree) {
  const sources = new Map();
  sources.empty = [];
  for (const [owner, relative] of Object.entries(FILES)) {
    const file = path.join(tree, relative);
    if (!fs.existsSync(file)) continue;
    if (fs.statSync(file).size === 0) {
      sources.empty.push(owner);
      continue;
    }
    const text = new String(fs.readFileSync(file, "utf8"));
    text.declared = declaredMethods(String(text));
    sources.set(owner, text);
  }
  return sources;
}

const sortKeys = (table) => Object.fromEntries(Object.entries(table).sort(([a], [b]) => a.localeCompare(b)));
const finishDecisions = (decisions, sites) => ({
  decisions: sortKeys(Object.fromEntries(Object.entries(decisions).map(([reference, rows]) => [reference, rows.map(({ from, list }) => ({ from, decisions: list }))]))),
  simplification: sites.map(({ from, list }) => ({ from, sites: list })),
});

function legacySection(present) {
  const root = legacyTrees();
  const decisions = {};
  const sites = [];
  const declarations = {};
  const releases = Object.keys(LEGACY);
  const absent = releases.filter((version) => !fs.existsSync(path.join(root, version, "net", "minecraft")));
  if (absent.length > 0) {
    // keeping the committed section is a cached run's to say (scripts/generate-all.cjs --cached), never a fresh one's
    if (!process.argv.includes("--cached")) throw new Error(`no legacy tree of ${absent.join(", ")} under ${root} (MC_LEGACY_TREES): run scripts/materialize-sources.cjs`);
    return present?.legacy ?? null;
  }
  for (const version of releases) {
    const sources = readSources(path.join(root, version));
    for (const references of Object.values(LEGACY[version])) {
      for (const reference of references) {
        const [owner, name] = reference.split("#");
        const source = sources.get(owner);
        const found = source ? declared(source, name) : [];
        if (found.length === 0) throw new Error(`${version} has no ${reference}`);
        const fingerprint = sha256(found.map((method) => method.source).join("\n"));
        const row = (declarations[reference] ??= { eras: [], helpers: [] });
        if (row.eras.at(-1)?.sha256 !== fingerprint) row.eras.push({ from: version, sha256: fingerprint });
        row.helpers = helperChain(reference, sources);
      }
    }
    keepDecisions(decisions, sites, version, sources);
  }
  return { releases, primitives: LEGACY, declarations: sortKeys(declarations), ...finishDecisions(decisions, sites) };
}

function generate() {
  const versions = registeredReleases().map(({ version }) => version);
  const declarations = {};
  const decisions = {};
  const simplification = [];
  const legacy = [];
  const unavailable = [];
  const unread = {};
  const present = fs.existsSync(outputPath) ? JSON.parse(fs.readFileSync(outputPath, "utf8")) : null;
  for (const version of versions) {
    if (compareReleases(version, "1.14") < 0) {
      legacy.push(version);
      continue;
    }
    const tree = modernTree(version);
    if (!tree) {
      unavailable.push(version);
      continue;
    }
    const sources = readSources(tree);
    if (sources.empty.length > 0) unread[version] = sources.empty;
    for (const entry of Object.values(PRIMITIVES)) {
      if (!Array.isArray(entry)) continue;
      for (const reference of entry) {
        const [owner, name] = reference.split("#");
        if (sources.empty.includes(owner)) continue;
        const source = sources.get(owner);
        const found = source ? declared(source, name) : [];
        const fingerprint = found.length === 0 ? null : sha256(found.map((method) => method.source).join("\n"));
        const row = (declarations[reference] ??= { eras: [], helpers: {} });
        const last = row.eras.at(-1);
        if (!last || last.sha256 !== fingerprint) row.eras.push({ from: version, sha256: fingerprint });
        if (found.length > 0) row.helpers = helperChain(reference, sources);
      }
    }
    keepDecisions(decisions, simplification, version, sources);
  }
  return {
    schema: 1,
    generatedBy: "scripts/generate-primitive-map.cjs",
    mapped: versions.filter((version) => !legacy.includes(version) && !unavailable.includes(version)),
    unavailable,
    unread,
    primitives: PRIMITIVES,
    declarations: sortKeys(declarations),
    ...finishDecisions(decisions, simplification),
    legacy: legacySection(present),
  };
}

function main() {
  const missing = nativePrimitives().filter((name) => !PRIMITIVES[name]);
  if (missing.length > 0) throw new Error(`Native primitives with no entry: ${missing.join(", ")}`);
  const generated = generate();
  if (generated.unavailable.length > 0 && !process.argv.includes("--cached")) throw new Error(`no exact source of ${generated.unavailable.join(", ")}`);
  const output = `${JSON.stringify(generated, null, 1)}\n`;
  if (process.argv.includes("--check")) {
    const present = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, "utf8").replaceAll("\r\n", "\n") : "";
    if (present !== output) throw new Error("generated/primitive-map.json differs from a fresh generation");
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

module.exports = { PRIMITIVES, LEGACY, DECISION_METHODS, nativePrimitives, helperChain, declaredMethods, decisionsOf };
