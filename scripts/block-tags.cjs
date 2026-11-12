// A release's block tags as its default data pack has them: its data/minecraft/tags JSON where the
// tree has resources, else its tag providers, every overload (`this.tag(BlockTags.X, ItemTags.Y)`,
// `this.tag(BlockItemTags.X)`) and symbolic id read, the trade-rebalance and update-pack providers
// (experimental packs, not the default one) left out. A tag filled by a filter over the registry
// (BlockTags.REPLACEABLE from 26.2) has only the members the provider names.
const fs = require("node:fs");
const path = require("node:path");
const { hoistStaticFields } = require("./java-source.cjs");

const EXPERIMENTAL_PROVIDER = /^(?:TradeRebalance|Update|Winter|Bundle|Minecart)/;

function tagFiles(directory, prefix = "") {
  const files = [];
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) files.push(...tagFiles(path.join(directory, entry.name), `${prefix}${entry.name}/`));
    else if (entry.name.endsWith(".json")) files.push([`${prefix}${entry.name.slice(0, -".json".length)}`, path.join(directory, entry.name)]);
  }
  return files;
}

/** The data/minecraft/tags of a release's tree, tag id -> members, null without resources. */
function resourceBlockTags(tree) {
  const directory = ["block", "blocks"]
    .map((name) => path.join(tree, "data", "minecraft", "tags", name))
    .find((candidate) => fs.existsSync(candidate));
  if (!directory) return null;
  const tags = new Map();
  for (const [id, file] of tagFiles(directory)) {
    const membership = { blocks: new Set(), includes: new Set() };
    for (const value of JSON.parse(fs.readFileSync(file, "utf8")).values ?? []) {
      const name = typeof value === "string" ? value : value.id;
      if (name.startsWith("#")) membership.includes.add(name.slice(1).replace(/^minecraft:/, ""));
      else membership.blocks.add(name.replace(/^minecraft:/, ""));
    }
    tags.set(id, membership);
  }
  return tags;
}

/** BlockTags' fields, field -> tag id: their create/bind names, or BlockItemTags' from 26.2. */
function blockTagFields(sourceRoot) {
  const tagsRoot = path.join(sourceRoot, "net", "minecraft", "tags");
  const fields = new Map();
  const shared = new Map();
  const sharedPath = path.join(tagsRoot, "BlockItemTags.java");
  if (fs.existsSync(sharedPath)) {
    for (const match of hoistStaticFields(fs.readFileSync(sharedPath, "utf8")).matchAll(/\b([A-Z0-9_]+)\s*=\s*BlockItemTagId\.create\("([^"]+)"/g)) {
      shared.set(match[1], match[2]);
    }
  }
  const source = hoistStaticFields(fs.readFileSync(path.join(tagsRoot, "BlockTags.java"), "utf8"));
  for (const match of source.matchAll(/static\s+final\s+[A-Za-z0-9_<>.]+<Block>\s+([A-Z0-9_]+)\s*=\s*(?:(?:create|bind)\("([^"]+)"\)|BlockItemTags\.([A-Z0-9_]+)\.block\(\))/g)) {
    const id = match[2] ?? shared.get(match[3]);
    if (id) fields.set(match[1], id);
  }
  for (const [field, id] of shared) if (!fields.has(field)) fields.set(field, id);
  return fields;
}

/** The tag providers' block memberships: tag id -> { blocks, includes }. */
function providerBlockTags(sourceRoot, fields, references) {
  const root = path.join(sourceRoot, "net", "minecraft", "data", "tags");
  const tags = new Map([...fields.values()].map((id) => [id, { blocks: new Set(), includes: new Set() }]));
  const files = fs.readdirSync(root, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".java") && !EXPERIMENTAL_PROVIDER.test(entry.name))
    .map((entry) => path.join(root, entry.name))
    .sort();
  const symbolNames = (symbol) => {
    const entry = references?.[symbol];
    if (!entry) return [];
    return entry.blocks ?? [...entry.weathering, ...entry.waxed];
  };
  for (const file of files) {
    const source = fs.readFileSync(file, "utf8");
    const locals = new Map();
    for (const match of source.matchAll(/\b([a-z][A-Za-z0-9]*)\s*=\s*new\s+\w+\[\]\s*\{([^}]*)\}/g)) locals.set(match[1], match[2]);
    for (const match of source.matchAll(/this\.tag\((?:BlockTags|BlockItemTags)\.([A-Z0-9_]+)(?:\s*,\s*ItemTags\.[A-Z0-9_]+)?\)([\s\S]*?);/g)) {
      const id = fields.get(match[1]);
      const membership = tags.get(id);
      if (!membership) continue;
      const body = match[2].replace(/\.add\(\s*([a-z][A-Za-z0-9]*)\s*\)/g, (text, local) => locals.get(local) ?? text);
      for (const block of body.matchAll(/\bBlocks\.([A-Z0-9_]+)/g)) membership.blocks.add(block[1].toLowerCase());
      for (const symbol of body.matchAll(/\b(?:BlockItemIds|BlockIds)\.[A-Z0-9_]+/g)) {
        for (const name of symbolNames(symbol[0])) membership.blocks.add(name);
      }
      for (const included of body.matchAll(/\b(?:BlockTags|BlockItemTags)\.([A-Z0-9_]+)/g)) {
        const other = fields.get(included[1]);
        if (other && other !== id) membership.includes.add(other);
      }
    }
  }
  return tags;
}

module.exports = { resourceBlockTags, blockTagFields, providerBlockTags };
