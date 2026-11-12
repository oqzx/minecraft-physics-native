// Writes generated/fluid-tables.json: for every registered release, its fluids, its fluid tags with their nesting
// resolved, the states of a fluid block, and the blocks that hold water, out of the release's own sources (the tree
// scripts/generate-block-friction.cjs picks). A release with no exact source is listed as unverified, with no table:
// a neighbour's is never taken for it, and its previous entry stays only when its sources are absent from this machine.
// An unresolved reference, a duplicate id or a method of a shape not known here fails the run. Pass --check to compare
// instead of writing: it fails when the output differs.

const fs = require("node:fs");
const path = require("node:path");
const { registeredReleases } = require("./registered-releases.cjs");
const { treeOf } = require("./generate-block-friction.cjs");
const { blockReferences, symbolicRegistrations, literalRegistrations } = require("./block-references.cjs");
const { javaCode, matchingBracket, hoistStaticFields } = require("./java-source.cjs");
const { sha256 } = require("./block-friction.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.join(repositoryRoot, "generated", "fluid-tables.json");

// (a release with no exact source; since 1.8.8 is decompiled from its own jar, none)
const NO_EXACT_SOURCE = new Set();
// The one release whose block registrations cannot be followed: its fluids and tags are read, the rest is not.
const FLUIDS_AND_TAGS_ONLY = { "1.13.2": "its Block.java registers the blocks through locals, which the holders cannot follow" };
const FAMILIES = [path.join("net", "minecraft", "world", "level", "block"), path.join("net", "minecraft", "block")];
const MATERIALS = [path.join("net", "minecraft", "world", "level", "material"), path.join("net", "minecraft", "fluid")];
const TAG_ROOTS = ["tags", path.join("registry", "tag"), "tag"].map((part) => path.join("net", "minecraft", part));
const PROVIDERS = [
  ["data", "tags", "FluidTagsProvider.java"],
  ["data", "server", "FluidTagsProvider.java"],
  ["data", "server", "FluidTagProvider.java"],
  ["data", "server", "tag", "FluidTagProvider.java"],
  ["data", "server", "tag", "vanilla", "VanillaFluidTagProvider.java"],
  ["data", "server", "tag", "VanillaFluidTagProvider.java"],
].map((parts) => path.join("net", "minecraft", ...parts));
const PROPERTY_FILES = [
  path.join("net", "minecraft", "world", "level", "block", "state", "properties", "BlockStateProperties.java"),
  path.join("net", "minecraft", "state", "property", "Properties.java"),
];

const firstFile = (tree, files) => files.map((file) => [file, tree.read(file)]).find(([, source]) => source != null);

function required(tree, files, release, what) {
  const found = firstFile(tree, files);
  if (!found) throw new Error(`${release}: ${tree.label} has no ${what}`);
  return found;
}

/** Fluids.java: the registered ids, in order; a repeat is an error. */
function parseFluids(source, release) {
  const ids = [...source.matchAll(/\bregister\(\s*(?:"([a-z0-9_]+)"|FluidIds\.([A-Z0-9_]+))/g)]
    .map((match) => match[1] ?? match[2].toLowerCase());
  if (ids.length === 0) throw new Error(`${release}: no fluid registrations`);
  if (new Set(ids).size !== ids.length) throw new Error(`${release}: a fluid registered twice: ${ids.join(", ")}`);
  return ids;
}

/** FluidTags.java: field -> tag id. */
function parseTagFields(source, release) {
  const fields = {};
  for (const match of source.matchAll(/\b([A-Z0-9_]+)\s*=\s*(?:[A-Za-z]+\.)?(?:create|register|bind|makeRequiredTag|of)\(\s*"([a-z0-9_/]+)"\s*\)/g)) {
    if (Object.values(fields).includes(match[2])) throw new Error(`${release}: fluid tag ${match[2]} declared twice`);
    fields[match[1]] = match[2];
  }
  if (Object.keys(fields).length === 0) throw new Error(`${release}: no fluid tags declared`);
  return fields;
}

/** A tag's JSON: its values as the file lists them, a nested tag keeping its #. */
function parseTagJson(text, id, release) {
  const values = JSON.parse(text).values;
  if (!Array.isArray(values)) throw new Error(`${release}: fluid tag ${id} has no values`);
  return values.map((value) => (typeof value === "string" ? value : value.id).replace("minecraft:", ""));
}

/** FluidTagsProvider's addTags: tag id -> values, a nested tag keeping its #. */
function parseProvider(source, fields, release) {
  const tags = {};
  const code = javaCode(source);
  for (const match of code.matchAll(/\b(?:tag|getOrCreateTagBuilder|valueLookupBuilder|getTagBuilder)\(\s*FluidTags\.([A-Z0-9_]+)\s*\)/g)) {
    const id = fields[match[1]];
    if (!id) throw new Error(`${release}: unresolved fluid tag ${match[1]}`);
    const end = code.indexOf(";", match.index);
    const chain = code.slice(match.index + match[0].length, end);
    const values = tags[id] ?? [];
    for (const call of chain.matchAll(/\.([A-Za-z]+)\(([^()]*)\)/g)) {
      for (const item of call[2].split(",").map((text) => text.trim()).filter(Boolean)) {
        const fluid = /^(?:Fluids|FluidIds)\.([A-Z0-9_]+)$/.exec(item);
        const nested = /^FluidTags\.([A-Z0-9_]+)$/.exec(item);
        if (fluid) values.push(fluid[1].toLowerCase());
        else if (nested && fields[nested[1]]) values.push(`#${fields[nested[1]]}`);
        else throw new Error(`${release}: unresolved member ${item} of fluid tag ${id}`);
      }
    }
    tags[id] = values;
  }
  return tags;
}

/** Each tag's members, its nested tags followed; a member no fluid or tag answers is an error, as is a cycle. */
function resolveTags(tags, fluids, release) {
  const known = new Set(fluids);
  const resolve = (id, seen) => {
    if (seen.includes(id)) throw new Error(`${release}: fluid tag cycle: ${[...seen, id].join(" > ")}`);
    if (!tags[id]) throw new Error(`${release}: unresolved fluid tag #${id}`);
    const members = new Set();
    for (const value of tags[id]) {
      if (value.startsWith("#")) for (const member of resolve(value.slice(1), [...seen, id])) members.add(member);
      else if (known.has(value)) members.add(value);
      else throw new Error(`${release}: fluid tag ${id} names unknown fluid ${value}`);
    }
    return [...members];
  };
  return Object.fromEntries(Object.keys(tags).sort().map((id) => [id, { values: tags[id], members: resolve(id, []).sort() }]));
}

/** A level property of the shared properties class, as [min, max]. */
function levelRange(properties, low, high, release) {
  const pattern = new RegExp(`=\\s*(?:IntegerProperty\\.create|IntProperty\\.of)\\(\\s*"level"\\s*,\\s*${low}\\s*,\\s*${high}\\s*\\)`);
  if (!pattern.test(properties)) throw new Error(`${release}: no level ${low}-${high} property`);
  return [low, high];
}

/**
 * LiquidBlock's stateCache as a block level reads it (1.13.2 FluidBlock.java, 26.3 LiquidBlock.java:48-57): 0 the
 * source, 1-7 flowing with amount 8 - level, 8 and above falling with amount 8, min(level, 8) being what getFluidState
 * looks up.
 */
function parseLevelStates(source, release) {
  const code = javaCode(source);
  const flowing = /\bgetFlowing\(\s*8\s*-\s*[a-z0-9_]+\s*,\s*false\s*\)/.test(code);
  const falling = /\bgetFlowing\(\s*8\s*,\s*true\s*\)/.test(code);
  const still = /\bget(?:Source|Still)\(\s*false\s*\)/.test(code);
  const clamp = /Math\.min\([^;]*,\s*8\s*\)/.test(code);
  if (!flowing || !falling || !still || !clamp) throw new Error(`${release}: the fluid block's stateCache is not the known one`);
  const states = [{ level: 0, amount: 8, source: true, falling: false }];
  for (let level = 1; level < 8; level += 1) states.push({ level, amount: 8 - level, source: false, falling: false });
  for (let level = 8; level < 16; level += 1) states.push({ level, amount: 8, source: false, falling: true });
  return states;
}

/** How a class's own getFluidState holds water: `inherent`, `waterlogged`, `fallingWaterlogged`, null for none, undefined with no such method. */
function classifyFluidState(file, source, release) {
  const code = javaCode(source);
  const declaration = /\bFluidState\s+getFluidState\s*\([^)]*\)\s*\{/.exec(code);
  if (!declaration) return undefined;
  const open = declaration.index + declaration[0].length - 1;
  const body = code.slice(open, matchingBracket(code, open));
  if (/\bLAVA\b/.test(body)) throw new Error(`${release}: ${file} holds lava`);
  const held = /Fluids\.WATER\.get(?:Source|Still)\(\s*(true|false)\s*\)/.exec(body);
  if (!held) {
    if (/stateCache|statesByLevel|\.fluidState\b|Fluids\.EMPTY|super\.getFluidState|\bgetFluidState\(\)|asBlockState|\.owner\b|\bfluid\b/.test(body)) return null;
    throw new Error(`${release}: ${file}: unknown getFluidState`);
  }
  const conditional = /\bWATERLOGGED\b/.test(body);
  if (!conditional && body.includes("?")) throw new Error(`${release}: ${file}: unknown condition in getFluidState`);
  return conditional ? (held[1] === "true" ? "fallingWaterlogged" : "waterlogged") : "inherent";
}

/** Every block id with its class, for the source shape of the tree. */
function registrationsOf(tree, family) {
  const blocks = tree.read(path.join(family, "Blocks.java"));
  const references = tree.root ? blockReferences(tree.root) : null;
  if (references) return symbolicRegistrations(blocks, references);
  // (net.minecraft.references as the tree has it, a directory or a pinned commit)
  const keys = {};
  for (const file of [["references", "Blocks"], ["references", "BlockIds"], ["block", "BlockKeys"]]) {
    const source = tree.read(`${path.join("net", "minecraft", ...file)}.java`);
    if (source == null) continue;
    for (const match of hoistStaticFields(source).matchAll(/\b([A-Z0-9_]+)\s*=\s*(?:createKey|of)\(\s*"([^"]+)"\s*\)/g)) keys[match[1]] = match[2];
  }
  return literalRegistrations(blocks.replaceAll("BlockKeys.", "Blocks."), keys);
}

/** The holders' block ids: each class that says how it holds water, and down the classes extending it. */
function waterHolders(tree, family, release) {
  const registrations = registrationsOf(tree, family);
  const kinds = { inherent: new Set(), waterlogged: new Set(), fallingWaterlogged: new Set() };
  const declared = new Map();
  // (the classes under state/ only pass the question on)
  for (const file of tree.grep("FluidState +getFluidState *\\(", family).filter((name) => path.normalize(path.dirname(name)) === family)) {
    const name = path.basename(file, ".java");
    const kind = classifyFluidState(file, tree.read(file) ?? "", release);
    if (kind !== undefined) declared.set(name, kind);
  }
  for (const [name, kind] of declared) if (kind) kinds[kind].add(name);
  for (const classes of Object.values(kinds)) {
    let frontier = [...classes];
    while (frontier.length > 0) {
      const found = tree.grep(`class +[A-Za-z0-9_]+(<[^>]*>)? +extends +(${frontier.join("|")})( |<|$)`, family)
        .map((file) => path.basename(file, ".java"))
        .filter((name) => !classes.has(name) && !declared.has(name));
      for (const name of found) classes.add(name);
      frontier = found;
    }
  }
  const holders = { inherent: [], waterlogged: [], fallingWaterlogged: [] };
  for (const [id, { className }] of Object.entries(registrations)) {
    for (const [kind, classes] of Object.entries(kinds)) if (classes.has(className)) holders[kind].push(id);
  }
  for (const list of Object.values(holders)) list.sort();
  if (holders.inherent.length === 0) throw new Error(`${release}: no block holds water of its own`);
  return holders;
}

const LEGACY_LIQUIDS = /\bregister\w*\(\s*\d+\s*,\s*"((?:flowing_)?(?:water|lava))"/g;

/** A release before fluids were a registry: the four liquid blocks of its Block.java, whose level is the metadata. */
function legacyTable(tree, file, source, release) {
  const ids = [...source.matchAll(LEGACY_LIQUIDS)].map((match) => match[1]);
  if (ids.length !== 4 || new Set(ids).size !== 4) throw new Error(`${release}: expected the four liquid blocks, found ${ids.join(", ")}`);
  return {
    source: tree.identity,
    files: [file.replaceAll("\\", "/")],
    sha256: sha256(source.replaceAll("\r\n", "\n")),
    legacy: true,
    liquidBlocks: ids.sort(),
    levels: { block: [0, 15] },
  };
}

function tableOf(tree, release) {
  for (const family of FAMILIES) {
    const file = path.join(family, "Block.java");
    const legacySource = tree.read(file);
    if (legacySource != null && new RegExp(LEGACY_LIQUIDS.source).test(legacySource)) return legacyTable(tree, file, legacySource, release);
  }
  const family = FAMILIES.find((candidate) => tree.read(path.join(candidate, "Blocks.java")) != null);
  if (!family) throw new Error(`${release}: ${tree.label} has no Blocks.java`);
  const [fluidsFile, fluidsSource] = required(tree, MATERIALS.map((root) => path.join(root, "Fluids.java")), release, "Fluids.java");
  const fluids = parseFluids(fluidsSource, release);
  const [tagsFile, tagsSource] = required(tree, TAG_ROOTS.map((root) => path.join(root, "FluidTags.java")), release, "FluidTags.java");
  const fields = parseTagFields(tagsSource, release);
  const jsonOf = (id) => tree.read(path.join("data", "minecraft", "tags", "fluid", `${id}.json`))
    ?? tree.read(path.join("data", "minecraft", "tags", "fluids", `${id}.json`));
  const fromResources = Object.values(fields).every((id) => jsonOf(id) != null);
  const provider = firstFile(tree, PROVIDERS);
  if (!fromResources && !provider) throw new Error(`${release}: ${tree.label} has neither the fluid tags' resources nor their provider`);
  const raw = fromResources
    ? Object.fromEntries(Object.values(fields).map((id) => [id, parseTagJson(jsonOf(id), id, release)]))
    : parseProvider(provider[1], fields, release);
  for (const id of Object.values(fields)) if (!raw[id]) throw new Error(`${release}: fluid tag ${id} is declared and never filled`);
  const partial = FLUIDS_AND_TAGS_ONLY[release];
  if (partial) {
    const inputs = [fluidsSource, tagsSource, JSON.stringify(raw)];
    return {
      source: tree.identity,
      files: [fluidsFile, tagsFile, "data/minecraft/tags"].map((file) => file.replaceAll("\\", "/")),
      sha256: sha256(inputs.map((text) => text.replaceAll("\r\n", "\n")).join("\n")),
      partial,
      fluids,
      tags: resolveTags(raw, fluids, release),
    };
  }
  const [liquidFile, liquidSource] = required(tree, [path.join(family, "LiquidBlock.java"), path.join(family, "FluidBlock.java")], release, "the fluid block");
  const [, properties] = required(tree, PROPERTY_FILES, release, "the shared properties");
  const inputs = [fluidsSource, tagsSource, liquidSource, fromResources ? JSON.stringify(raw) : provider[1]];
  return {
    source: tree.identity,
    files: [fluidsFile, tagsFile, liquidFile, fromResources ? "data/minecraft/tags" : provider[0]].map((file) => file.replaceAll("\\", "/")),
    sha256: sha256(inputs.map((text) => text.replaceAll("\r\n", "\n")).join("\n")),
    fluids,
    tags: resolveTags(raw, fluids, release),
    levels: { block: levelRange(properties, 0, 15, release), flowing: levelRange(properties, 1, 8, release) },
    states: parseLevelStates(liquidSource, release),
    holders: waterHolders(tree, family, release),
  };
}

function generate() {
  const previous = fs.existsSync(outputPath) ? JSON.parse(fs.readFileSync(outputPath, "utf8")).releases ?? {} : {};
  const releases = {};
  const unverified = [];
  for (const { version } of registeredReleases()) {
    const tree = NO_EXACT_SOURCE.has(version) ? undefined : treeOf(version);
    if (tree) releases[version] = tableOf(tree, version);
    else if (previous[version] && !NO_EXACT_SOURCE.has(version)) releases[version] = previous[version];
    else unverified.push(version);
  }
  return { schema: 1, generatedBy: "scripts/generate-fluid-tables.cjs", unverified, releases };
}

function main() {
  const output = `${JSON.stringify(generate(), null, 2)}\n`;
  if (process.argv.includes("--check")) {
    const current = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, "utf8").replaceAll("\r\n", "\n") : undefined;
    if (current !== output) throw new Error(`${outputPath} is out of date`);
    return;
  }
  fs.writeFileSync(outputPath, output);
}

if (require.main === module) main();

module.exports = { parseFluids, parseTagFields, parseProvider, parseTagJson, resolveTags, parseLevelStates, classifyFluidState, tableOf, generate };
