// Blocks.java from 26.2 names its blocks by symbol: BlockItemIds.WHEAT_CROP is the block "wheat",
// and BlockItemIds.BED sixteen beds. This resolves net.minecraft.references to the identifiers and
// walks Blocks' registrations, a registration it cannot read being an error, never an omission.
const fs = require("node:fs");
const path = require("node:path");
const { javaCode, matchingBracket, splitTopLevel, hoistStaticFields } = require("./java-source.cjs");

// ColorCollection's record components (26.2 ColorCollection.java:15) as DyeColor names them.
const COLORS = [
  "white", "orange", "magenta", "light_blue", "yellow", "lime", "pink", "gray", "light_gray",
  "cyan", "purple", "blue", "brown", "green", "red", "black",
];
// WeatheringCopperCollection.PREFIXES: ByState's unaffected, exposed, weathered, oxidized.
const WEATHERING_PREFIXES = ["", "exposed_", "weathered_", "oxidized_"];
const WAXED_PREFIXES = WEATHERING_PREFIXES.map((prefix) => `waxed_${prefix}`);

const colored = (name) => COLORS.map((color) => `${color}_${name}`);
const weathering = (byState) => byState.map((name, index) => `${WEATHERING_PREFIXES[index]}${name}`);
const waxed = (byState) => byState.map((name, index) => `${WAXED_PREFIXES[index]}${name}`);
const copper = (byState) => ({ weathering: weathering(byState), waxed: waxed(byState) });

/** One field of BlockItemIds or BlockIds: its block names (`weathering`/`waxed` for copper). */
function fieldBlocks(initializer, stringLists) {
  const string = /^(?:BlockItemId\.)?create\(\s*"([^"]+)"(?:\s*,\s*"[^"]+")?\s*\)$/.exec(initializer);
  if (string) return { blocks: [string[1]] };
  const simpleColored = /^createSimpleColored\(\s*"([^"]+)"\s*\)$/.exec(initializer);
  if (simpleColored) return { blocks: colored(simpleColored[1]) };
  const simpleCopper = /^createSimpleCopper\(\s*"([^"]+)"\s*\)$/.exec(initializer);
  if (simpleCopper) return copper(Array(4).fill(simpleCopper[1]));
  const specialCopper = /^WeatheringCopperCollection\.prefixWithState\(\s*WeatheringCopperCollection\.same\(\s*([A-Z0-9_]+)\s*\)\s*\)/.exec(initializer);
  if (specialCopper && stringLists[specialCopper[1]]) return copper(stringLists[specialCopper[1]]);
  return null;
}

/** The fields of a references class, by symbol, across a statics block that assigns them. */
function referenceFields(source) {
  const fields = {};
  const stringLists = {};
  for (const match of source.matchAll(/\b([A-Z0-9_]+)\s*=\s*new\s+WeatheringCopperCollection\.ByState<String>\(([^)]*)\)/g)) {
    stringLists[match[1]] = [...match[2].matchAll(/"([^"]+)"/g)].map((name) => name[1]);
  }
  for (const match of source.matchAll(/(?:^|[\s>])([A-Z0-9_]+)\s*=\s*([^;]+);/gm)) {
    const initializer = match[2].replace(/\s+/g, " ").trim();
    if (initializer.startsWith("new WeatheringCopperCollection.ByState")) continue;
    if (!/create|prefixWithState/.test(initializer)) continue;
    const blocks = fieldBlocks(initializer, stringLists);
    if (!blocks) {
      if (/BlockItemId|createSimple|WeatheringCopperCollection/.test(initializer) || /^create\(/.test(initializer)) {
        throw new Error(`Unresolved block identifier ${match[1]}: ${initializer}`);
      }
      continue;
    }
    fields[match[1]] = blocks;
  }
  return fields;
}

/** BlockItemIds and BlockIds of a release's sources, `BlockItemIds.X` and `BlockIds.X` as keys. */
function blockReferences(tree) {
  const references = {};
  for (const name of ["BlockItemIds", "BlockIds"]) {
    const file = path.join(tree, "net", "minecraft", "references", `${name}.java`);
    if (!fs.existsSync(file)) return null;
    for (const [field, blocks] of Object.entries(referenceFields(fs.readFileSync(file, "utf8")))) {
      references[`${name}.${field}`] = blocks;
    }
  }
  return references;
}

function reference(references, text) {
  const symbol = /(?:BlockItemIds|BlockIds)\.[A-Z0-9_]+/.exec(text)?.[0];
  const blocks = symbol ? references[symbol] : undefined;
  if (!blocks) throw new Error(`Unresolved block reference: ${text.slice(0, 120)}`);
  return blocks;
}

const namedClass = (text) => /\bnew\s+([A-Za-z0-9_]+)\s*\(|\b([A-Za-z0-9_]+)::new\b/.exec(text)?.slice(1).find(Boolean) ?? null;

/** What a Blocks.register's factory argument makes: its class, Block for bare properties. */
function factoryClass(args) {
  if (args.length < 3) return "Block";
  const named = namedClass(args[1]);
  if (!named) throw new Error(`Unresolved block factory: ${args[1].slice(0, 120)}`);
  return named;
}

const HELPER_CLASSES = {
  registerLegacyStair: "StairBlock",
  registerStair: "StairBlock",
  registerSlab: "SlabBlock",
  registerWall: "WallBlock",
};

/** Every block of Blocks' static initializer: id -> { field, className, initializer, confidence }. */
function symbolicRegistrations(blocksSource, references) {
  const code = javaCode(blocksSource);
  const classes = {};
  const add = (id, field, className, initializer) => {
    if (classes[id]) throw new Error(`Duplicate block registration: ${id}`);
    classes[id] = { field, className, initializer: initializer.replace(/\s+/g, " ").trim(), confidence: "source" };
  };
  for (const match of code.matchAll(/^\s+([A-Z0-9_]+)\s*=\s*([A-Za-z.]+)\(/gm)) {
    const [, field, callee] = match;
    const open = match.index + match[0].length - 1;
    const close = matchingBracket(code, open);
    const text = blocksSource.slice(open + 1, close);
    const args = splitTopLevel(code.slice(open + 1, close)).map((argument) => {
      const at = code.indexOf(argument, open);
      return blocksSource.slice(at, at + argument.length);
    });
    if (callee === "register") {
      const [id] = reference(references, args[0]).blocks;
      add(id, field, factoryClass(args), text);
    } else if (HELPER_CLASSES[callee]) {
      const [id] = reference(references, args[0]).blocks;
      add(id, field, HELPER_CLASSES[callee], text);
    } else if (callee === "ColorCollection.registerBlocks") {
      const className = namedClass(args[2]);
      if (!className) throw new Error(`Unresolved colored block factory: ${field}`);
      for (const id of reference(references, args[0]).blocks) add(id, field, className, text);
    } else if (callee === "WeatheringCopperCollection.registerBlocks") {
      const ids = reference(references, args[0]);
      const [waxedClass, weatheringClass] = [namedClass(args[2]), namedClass(args[3])];
      if (!ids.weathering || !waxedClass || !weatheringClass) throw new Error(`Unresolved copper block factory: ${field}`);
      for (const id of ids.weathering) add(id, field, weatheringClass, text);
      for (const id of ids.waxed) add(id, field, waxedClass, text);
    } else if (callee === "ColorCollection.zipMap") {
      const helper = /\b(register[A-Za-z]*)\(/.exec(args[2])?.[1];
      if (!HELPER_CLASSES[helper]) throw new Error(`Unresolved colored block helper: ${field}`);
      for (const id of reference(references, args[1]).blocks) add(id, field, HELPER_CLASSES[helper], text);
    } else if (/^[A-Z]/.test(callee) || /^register/.test(callee)) {
      throw new Error(`Unresolved block registration ${field}: ${callee}`);
    }
  }
  return classes;
}

// Helpers of Blocks that make one class, for a body that does not say it plainly.
const LITERAL_HELPER_CLASSES = {
  ...HELPER_CLASSES,
  registerBed: "BedBlock",
  bed: "BedBlock",
  registerStainedGlass: "StainedGlassBlock",
  stainedGlass: "StainedGlassBlock",
  log: "RotatedPillarBlock",
  netherStem: "RotatedPillarBlock",
  leaves: "LeavesBlock",
  woodenButton: "ButtonBlock",
  stoneButton: "ButtonBlock",
  flowerPot: "FlowerPotBlock",
  shulkerBox: "ShulkerBoxBlock",
  pistonBase: "PistonBaseBlock",
  legacyStair: "StairBlock",
  registerOldStairsBlock: "StairsBlock",
  registerStairsBlock: "StairsBlock",
};

/** The class a static helper of Blocks makes, by helper name. */
function helperClasses(source) {
  const code = javaCode(source);
  const helpers = {};
  for (const match of code.matchAll(/\bprivate\s+static\s+[\w<>?, .]+\s+([A-Za-z0-9_]+)\s*\([^)]*\)\s*\{/g)) {
    const open = match.index + match[0].length - 1;
    const name = namedClass(code.slice(open, matchingBracket(code, open)));
    if (name) helpers[match[1]] = name;
  }
  return { ...helpers, ...LITERAL_HELPER_CLASSES };
}

/** The keys of net.minecraft.references (Blocks from 1.20.3, BlockIds in 26.1): field -> id. */
function referenceBlockKeys(tree) {
  const keys = {};
  for (const name of ["Blocks", "BlockIds"]) {
    const file = path.join(tree, "net", "minecraft", "references", `${name}.java`);
    if (!fs.existsSync(file)) continue;
    for (const match of hoistStaticFields(fs.readFileSync(file, "utf8")).matchAll(/\b([A-Z0-9_]+)\s*=\s*createKey\(\s*"([^"]+)"\s*\)/g)) {
      keys[match[1]] = match[2];
    }
  }
  return keys;
}

/**
 * Every `Block X = register...("id", ...)` of a Blocks.java, at its declaration or in its static
 * block: id -> { field, className, initializer, confidence }, the id a string or a references key.
 * `WeatheringCopperBlocks.create("name", Blocks::register, Waxed::new, Weathering::new, ...)` (1.21.9
 * to 26.1.2) is its eight blocks. A field it cannot read is an error naming the field, never a
 * block fewer.
 */
function literalRegistrations(source, keys = {}) {
  const code = javaCode(source);
  const helpers = helperClasses(source);
  const classes = {};
  for (const match of code.matchAll(/(?:\bstatic\s+final\s+(?:Block|WeatheringCopperBlocks)\s+|^[ \t]+)([A-Z0-9_]+)\s*=\s*([A-Za-z_.]+)\s*\(/gm)) {
    const [, field, callee] = match;
    const open = match.index + match[0].length - 1;
    const close = matchingBracket(code, open);
    if (close < 0) throw new Error(`Unbalanced block registration ${field}`);
    const args = splitTopLevel(source.slice(open + 1, close));
    if (callee === "WeatheringCopperBlocks.create") {
      const base = /^"([a-z0-9_]+)"$/.exec(args[0])?.[1];
      const [waxedClass, weatheringClass] = [namedClass(args[2]), namedClass(args[3])];
      if (!base || !waxedClass || !weatheringClass) throw new Error(`Unresolved copper block factory: ${field}`);
      const add = (id, className) => {
        if (classes[id]) throw new Error(`Duplicate block registration: ${id}`);
        classes[id] = { field, className, initializer: args.join(", ").replace(/\s+/g, " "), confidence: "source" };
      };
      for (const prefix of WEATHERING_PREFIXES) add(`${prefix}${base}`, weatheringClass);
      for (const prefix of WAXED_PREFIXES) add(`${prefix}${base}`, waxedClass);
      continue;
    }
    if (!/^register/.test(callee) && !helpers[callee]) throw new Error(`Unresolved block registration ${field}: ${callee}`);
    const first = args[0].replace(/^\((?:String|ResourceKey(?:<Block>)?)\)\s*/, "");
    const symbol = /^(?:net\.minecraft\.references\.)?(?:Blocks|BlockIds)\.([A-Z0-9_]+)$/.exec(first)?.[1];
    const id = /^"([a-z0-9_./-]+)"$/.exec(first)?.[1] ?? keys[symbol];
    if (!id) throw new Error(`Unresolved block identifier ${field}: ${args[0].slice(0, 120)}`);
    if (classes[id]) throw new Error(`Duplicate block registration: ${id}`);
    const rest = args.slice(1).join(", ");
    const helper = /^([A-Za-z0-9_]+)\s*\(/.exec(rest.trim())?.[1];
    const className = LITERAL_HELPER_CLASSES[callee]
      ?? namedClass(javaCode(rest))
      ?? (helper ? helpers[helper] : null)
      ?? (/^(?:[A-Za-z]+\.)*Properties\.|^[A-Za-z]+Properties\(|^[a-z][A-Za-z0-9]*$/.test(rest.trim()) ? "Block" : null);
    classes[id] = {
      field,
      className,
      initializer: rest.replace(/\s+/g, " ").trim(),
      confidence: className ? "source" : "unresolved",
    };
  }
  return classes;
}

module.exports = { blockReferences, symbolicRegistrations, literalRegistrations, referenceBlockKeys, referenceFields, COLORS };
