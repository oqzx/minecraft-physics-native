// Whether the release inventory is the same exact names everywhere and each release's data is sound
// (scripts/verify-native-package.cjs, pnpm verify:release-data). Equal counts prove nothing: every
// list is compared name for name, and each table is read back, field by field, against its own
// record of the source. The schema boundary is the six legacy releases (1.8.8 - 1.13.2, 1.13.2 being
// the flattened one) against the modern ones from 1.14.4.
//
// This holds the data to itself and to the exact-source records; it does not prove extraction is
// complete. That is the running-client registries' job (scripts/release-admission.cjs).
const fs = require("node:fs");
const path = require("node:path");
const { compareReleases, registeredReleases } = require("./registered-releases.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const prefixOf = (version) => `V${version.replaceAll(".", "_")}`;
const moduleOf = (version) => `v${version.replaceAll(".", "_")}`;

const stringList = (text) => [...text.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((match) => match[1]);
const field = (record, name) => new RegExp(`\\b${name}: ([^,}]+?)\\s*(?:,|$)`).exec(record)?.[1]?.trim();
const stringField = (record, name) => /^"(.*)"$/.exec(field(record, name) ?? "")?.[1];
const records = (text, type) => [...text.matchAll(new RegExp(`^\\s*${type} \\{(.*)\\},?\\s*$`, "gm"))].map((match) => match[1]);

/** The pieces of one native/data/releases/*.rs that matter here. */
function parseTable(source) {
  const text = source.replaceAll("\r\n", "\n");
  const lists = new Map();
  for (const [, name, body] of text.matchAll(/^pub static (\w+): &\[&str\] = &\[(.*)\];$/gm)) lists.set(name, stringList(body));
  const domains = new Map();
  for (const [, name, body] of text.matchAll(/^pub static (\w+): &\[&\[&str\]\] = &\[(.*)\];$/gm)) {
    domains.set(name, body.split(",").map((entry) => entry.trim()).filter(Boolean));
  }
  const counts = new Map();
  for (const [, name, count] of text.matchAll(/^pub static (\w+): usize = (\d+);$/gm)) counts.set(name, Number(count));
  const properties = new Map();
  for (const [, name, inline, body] of text.matchAll(/^pub static (\w+): &\[GeneratedProperty<'static>\] = &\[(\];|\n([\s\S]*?)^\];)/gm)) {
    properties.set(name, inline === "];" ? [] : records(body, "GeneratedProperty"));
  }
  return {
    statics: [...text.matchAll(/^pub static (\w+):/gm)].map((match) => match[1]),
    lists, domains, counts, properties,
    blocks: records(text, "GeneratedBlock"),
    fluids: records(text, "GeneratedFluid"),
    fluidTags: records(text, "GeneratedFluidTag"),
    blockTags: records(text, "GeneratedBlockTag"),
  };
}

// The enum constants serialized under another name than their own, as each enum declares them
// (1.12.2 net/minecraft/block/WallBlock.java and the like; 1.13.2 block/enums/PistonType.java, the
// same in the Mojang-named trees).
const SERIALIZED_CONSTANTS = {
  "PistonType.DEFAULT": "normal",
  "PistonHeadBlock.PistonHeadType.DEFAULT": "normal",
  "WallBlock.WallType.NORMAL": "cobblestone",
  "PrismarineBlock.PrismarineType.ROUGH": "prismarine",
  "RedSandstoneBlock.RedSandstoneType.DEFAULT": "red_sandstone",
  "SandstoneBlock.SandstoneType.DEFAULT": "sandstone",
  "StoneBrickBlock.Type.DEFAULT": "stonebrick",
};

/** A property default as the source writes it, in the terms of the property's values. */
function defaultInDomain(value, values) {
  if (SERIALIZED_CONSTANTS[value]) return values.includes(SERIALIZED_CONSTANTS[value]);
  if (/^-?\d+$/.test(value) || value === "true" || value === "false") return values.includes(value);
  const constant = /^(?:[\w$]+\.)*([A-Z][A-Z0-9_]*)$/.exec(value)?.[1];
  // what is no plain constant (a field of the block, a computed value) has no value to hold to
  return constant === undefined ? true : values.map((entry) => entry.toLowerCase()).includes(constant.toLowerCase());
}

const REQUIRED_FLUIDS = ["water", "flowing_water", "lava", "flowing_lava"];

/** What is wrong with one release's table, each problem naming the release and the record. */
function tableProblems(version, text, { registry = null, legacy = false } = {}) {
  const table = parseTable(text);
  const prefix = `${prefixOf(version)}_`;
  const problems = [];
  const problem = (message) => problems.push(`${version}: ${message}`);

  for (const name of table.statics) {
    if (!name.startsWith(prefix)) problem(`the table holds ${name}, labelled for another release`);
  }
  const listed = (name, kind) => {
    const list = table.lists.get(name);
    if (!name?.startsWith(prefix)) problem(`${kind} points at ${name}, labelled for another release`);
    else if (!list) problem(`${kind} points at ${name}, which the table does not hold`);
    return list ?? [];
  };

  if (table.blocks.length === 0) problem("the table holds no blocks");
  const ids = new Set();
  for (const record of table.blocks) {
    const id = stringField(record, "id");
    if (ids.has(id)) problem(`block ${id} is listed twice`);
    ids.add(id);
    const propertiesName = field(record, "properties");
    const properties = table.properties.get(propertiesName);
    if (!properties) {
      problem(`block ${id} points at ${propertiesName}, which the table does not hold`);
      continue;
    }
    const order = listed(field(record, "property_order"), `block ${id}'s property order`);
    const names = properties.map((property) => stringField(property, "name"));
    if ([...order].sort().join() !== [...names].sort().join()) problem(`block ${id} orders ${order.join() || "nothing"} over ${names.join() || "nothing"}`);
    let states = 1;
    for (const property of properties) {
      const name = stringField(property, "name");
      const values = listed(field(property, "values"), `block ${id}.${name}`);
      if (values.length === 0) problem(`block ${id}.${name} has an empty domain`);
      if (new Set(values).size !== values.length) problem(`block ${id}.${name} repeats a value of ${values.join()}`);
      for (const value of listed(field(property, "defaults"), `block ${id}.${name} defaults`)) {
        if (!defaultInDomain(value, values)) problem(`block ${id}.${name} defaults to ${value}, outside ${values.join()}`);
      }
      states *= values.length;
    }
    const count = /^Some\((\d+)\)$/.exec(field(record, "state_count") ?? "")?.[1];
    if (count === undefined) problem(`block ${id} has no state count`);
    else if (Number(count) !== states) problem(`block ${id} has ${count} states over a domain of ${states}`);
  }
  if (registry) {
    for (const id of registry) if (!ids.has(id)) problem(`block ${id} of the source registry is not in the table`);
    const known = new Set(registry);
    for (const id of ids) if (!known.has(id)) problem(`block ${id} of the table is not in the source registry`);
  }

  const fluids = new Map(table.fluids.map((record) => [stringField(record, "id"), record]));
  for (const id of REQUIRED_FLUIDS) if (!fluids.has(id)) problem(`required fluid ${id} is absent`);
  for (const [id, record] of fluids) {
    const domainsName = field(record, "property_domains");
    const domainNames = table.domains.get(domainsName);
    if (!domainNames) {
      problem(`fluid ${id} points at ${domainsName}, which the table does not hold`);
      continue;
    }
    const domains = domainNames.map((name) => listed(name, `fluid ${id}'s domain`));
    const names = listed(field(record, "property_names"), `fluid ${id}'s properties`);
    if (names.length !== domains.length) problem(`fluid ${id} has ${names.length} properties over ${domains.length} domains`);
    const expected = domains.reduce((product, domain) => product * domain.length, 1);
    const countName = field(record, "state_count");
    const count = table.counts.get(countName);
    if (count === undefined) problem(`fluid ${id} points at ${countName}, which the table does not hold`);
    else if (count !== expected) problem(`fluid ${id} has ${count} states over a domain of ${expected}`);
    for (const name of ["tick_delay", "normal_drop_off"]) {
      if (!/^Some\(\d+\)$/.test(field(record, name) ?? "")) problem(`fluid ${id} has no ${name}`);
    }
  }
  if (!legacy) {
    const tags = table.fluidTags.map((record) => stringField(record, "id"));
    for (const id of ["water", "lava"]) if (!tags.includes(id)) problem(`fluid tag ${id} is absent`);
  }
  const tagIds = new Set(table.blockTags.map((record) => stringField(record, "id")));
  for (const record of table.blockTags) {
    const tag = stringField(record, "id");
    for (const block of listed(field(record, "blocks"), `tag ${tag}`)) {
      if (!ids.has(block)) problem(`tag ${tag} names block ${block}, which the table does not hold`);
    }
    for (const included of listed(field(record, "includes"), `tag ${tag}'s includes`)) {
      if (!tagIds.has(included)) problem(`tag ${tag} includes ${included}, which the table does not hold`);
    }
  }
  return problems;
}

function difference(problems, leftName, left, rightName, right) {
  const rightSet = new Set(right);
  const leftSet = new Set(left);
  for (const name of leftSet) if (!rightSet.has(name)) problems.push(`${name} is in ${leftName} and not in ${rightName}`);
  for (const name of rightSet) if (!leftSet.has(name)) problems.push(`${name} is in ${rightName} and not in ${leftName}`);
  for (const [listName, list] of [[leftName, left], [rightName, right]]) {
    for (const name of new Set(list.filter((entry, index) => list.indexOf(entry) !== index))) problems.push(`${name} is twice in ${listName}`);
  }
}

/**
 * The exact-name comparison of every inventory: `modern` and `legacy` are the source manifests'
 * names, the rest each list of the same releases.
 */
function inventoryProblems({ modern, legacy, registered, tables, tableRegistrations, typescript, manifest, blockStates, blockRegistries }) {
  const problems = [];
  const all = [...legacy, ...modern].sort(compareReleases);
  for (const name of legacy) if (modern.includes(name)) problems.push(`${name} is both legacy and modern`);
  difference(problems, "the source manifests", all, "native/src/versioned/mod.rs", registered);
  difference(problems, "the source manifests", all, "native/data/releases", tables);
  difference(problems, "the source manifests", all, "the GeneratedRelease list", tableRegistrations);
  difference(problems, "the source manifests", all, "src/versions.ts", typescript);
  difference(problems, "the source manifests", all, "generated/release-manifest.json", manifest);
  difference(problems, "the modern manifest", modern, "generated/block-state-definitions.json", blockStates);
  difference(problems, "the modern manifest", modern, "generated/block-registries.json", blockRegistries);
  return problems;
}

/** Each GeneratedRelease of native/data/mod.rs, against the module its file is. */
function registrationProblems(text) {
  const problems = [];
  const names = [];
  for (const [, record] of text.matchAll(/^\s*GeneratedRelease \{(.*)\},?\s*$/gm)) {
    const version = stringField(record, "version");
    names.push(version);
    for (const [name, suffix] of [["blocks", "BLOCKS"], ["fluids", "FLUIDS"], ["fluid_tags", "FLUID_TAGS"], ["block_tags", "BLOCK_TAGS"]]) {
      const pointed = field(record, name);
      if (pointed !== `${prefixOf(version)}_${suffix}`) problems.push(`${version}: its ${name} are ${pointed}`);
    }
    if (!text.includes(`#[path = "releases/${moduleOf(version)}.rs"]`)) problems.push(`${version}: native/data/mod.rs loads no releases/${moduleOf(version)}.rs`);
  }
  return { names, problems };
}

function legacyProblems(releases) {
  const problems = [];
  for (const [version, record] of Object.entries(releases)) {
    if (record.flattened !== (version === "1.13.2")) problems.push(`${version}: flattened is ${record.flattened}`);
    if (!record.sourceRelease) problems.push(`${version}: no source release`);
    if (!Array.isArray(record.registry?.blocks) || record.registry.blocks.length === 0) problems.push(`${version}: no block registry`);
  }
  return problems;
}

function typescriptReleases(root) {
  const text = fs.readFileSync(path.join(root, "src", "versions.ts"), "utf8");
  const list = /SUPPORTED_VERSIONS = \[([\s\S]*?)\]/.exec(text);
  if (!list) throw new Error("src/versions.ts has no SUPPORTED_VERSIONS");
  return [...list[1].matchAll(/['"]([^'"]+)['"]/g)].map((match) => match[1]);
}

/** Every problem of the data of a checkout; empty when the union is one. */
function releaseDataProblems(root = repositoryRoot) {
  const generated = (name) => JSON.parse(fs.readFileSync(path.join(root, "generated", name), "utf8"));
  const modern = generated("minecraft-versions.json").versions;
  const legacy = generated("legacy-release-data.json").releases;
  const states = generated("block-state-definitions.json").releases;
  const registries = generated("block-registries.json").versions;
  const tableRoot = path.join(root, "native", "data", "releases");
  const tableFiles = fs.readdirSync(tableRoot).filter((file) => file.endsWith(".rs"));
  const fileOf = new Map([...modern, ...Object.keys(legacy)].map((version) => [`${moduleOf(version)}.rs`, version]));
  const registrations = registrationProblems(fs.readFileSync(path.join(root, "native", "data", "mod.rs"), "utf8"));
  const problems = [
    ...inventoryProblems({
      modern,
      legacy: Object.keys(legacy),
      registered: registeredReleases(path.join(root, "native", "src", "versioned")).map(({ version }) => version),
      tables: tableFiles.map((file) => fileOf.get(file) ?? file),
      tableRegistrations: registrations.names,
      typescript: typescriptReleases(root),
      manifest: Object.keys(generated("release-manifest.json").releases),
      blockStates: Object.keys(states),
      blockRegistries: Object.keys(registries),
    }),
    ...registrations.problems,
    ...legacyProblems(legacy),
  ];
  for (const file of tableFiles) {
    const version = fileOf.get(file);
    if (!version) continue;
    const registry = legacy[version]?.registry?.blocks ?? registries[version]?.blocks ?? null;
    problems.push(...tableProblems(version, fs.readFileSync(path.join(tableRoot, file), "utf8"), { registry, legacy: Boolean(legacy[version]) }));
  }
  return problems;
}

/** The union against what a loaded binding and the public module dispatch. */
function dispatchProblems(binding, physics, versions) {
  const problems = [];
  difference(problems, "the source manifests", versions, "SUPPORTED_VERSIONS of the package", [...physics.SUPPORTED_VERSIONS]);
  difference(problems, "the source manifests", versions, "NATIVE_PHYSICS_ENGINES", Object.keys(physics.NATIVE_PHYSICS_ENGINES));
  for (const version of versions) {
    if (!binding.isSupportedVersion(version)) problems.push(`${version}: the binding refuses it`);
    if (physics.NATIVE_PHYSICS_ENGINES[version]?.version !== version) problems.push(`${version}: its engine class serves ${physics.NATIVE_PHYSICS_ENGINES[version]?.version}`);
    for (const fluid of ["water", "lava"]) {
      if (!(binding.generatedFluidStateCount(version, fluid) > 0)) problems.push(`${version}: the binding has no ${fluid} states`);
    }
    if (!(binding.generatedBlockStateCount(version, "stone") > 0)) problems.push(`${version}: the binding has no stone`);
  }
  for (const version of ["1.8", "1.14", "1.20.7", "26.4"]) {
    if (binding.isSupportedVersion(version)) problems.push(`${version}: the binding accepts a release outside the inventory`);
  }
  return problems;
}

if (require.main === module) {
  const problems = releaseDataProblems();
  for (const problem of problems) console.error(problem);
  if (problems.length > 0) process.exit(1);
  console.log("release data: names, schemas and tables agree");
}

module.exports = { parseTable, tableProblems, inventoryProblems, registrationProblems, releaseDataProblems, dispatchProblems };
