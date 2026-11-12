// The canonical release inventory: generated/minecraft-versions.json (the modern manifest releases,
// 1.14.4 on) joined with LEGACY_RELEASES (1.8.8 to 1.13.2). No other list of releases is hand-kept:
// the TypeScript registry is written from this one (generate-version-exports.cjs) and every other
// surface is held to it name by name, so a missing release and an extra one cannot cancel out as
// they could when two counts were compared. A problem names the release and the file.
const fs = require("node:fs");
const path = require("node:path");
const { LEGACY_RELEASES } = require("./legacy-release-data.cjs");
const { compareReleases } = require("./registered-releases.cjs");

const repositoryRoot = path.resolve(__dirname, "..");

const moduleOf = (version) => `v${version.replaceAll(".", "_")}`;
const repeated = (list) => [...new Set(list.filter((item, index) => list.indexOf(item) !== index))];

/** The two lists the inventory is made from, as their files have them. */
function inventorySources(root = repositoryRoot) {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, "generated", "minecraft-versions.json"), "utf8"));
  return { modern: manifest.versions, legacy: LEGACY_RELEASES.map((release) => release.version) };
}

/** The exact ids of every release, oldest first; duplicates are for inventoryProblems to report. */
function canonicalReleases(sources = inventorySources()) {
  return [...new Set([...sources.legacy, ...sources.modern])].sort(compareReleases);
}

/** What each surface declares, read with no assumption that the surfaces agree. */
function readSurfaces(root = repositoryRoot) {
  const versioned = path.join(root, "native", "src", "versioned");
  const registry = fs.readFileSync(path.join(versioned, "mod.rs"), "utf8");
  const modules = [...registry.matchAll(/#\[path = "(v-[^"]+\.rs)"\]\s*pub mod (\w+);/g)].map(([, file, name]) => {
    const target = path.join(versioned, file);
    const source = fs.existsSync(target) ? fs.readFileSync(target, "utf8") : "";
    return { file, name, label: /const VERSION: &'static str = "([^"]+)";/.exec(source)?.[1] ?? null };
  });
  const list = /pub const SUPPORTED_VERSIONS: &\[&str\] = &\[([\s\S]*?)\];/.exec(registry)?.[1] ?? "";
  const dataRoot = path.join(root, "native", "data", "releases");
  const dataFiles = fs.readdirSync(dataRoot).filter((entry) => entry.endsWith(".rs")).map((file) => {
    const prefix = /pub static (V[0-9_]+?)_[A-Z]/.exec(fs.readFileSync(path.join(dataRoot, file), "utf8"))?.[1] ?? null;
    return { file, prefix };
  });
  return {
    modules,
    supported: [...list.matchAll(/(\w+)::Engine::VERSION/g)].map((match) => match[1]),
    profiles: [...registry.matchAll(/(\w+)::Engine::VERSION => Some\(\1::Profile::rules\(\)\)/g)].map((match) => match[1]),
    dispatch: [...registry.matchAll(/(\w+)::Engine::VERSION => \{\s*tick_conforming::<\1::Engine, \1::Profile>/g)].map((match) => match[1]),
    dataFiles,
  };
}

/** Each difference between the inventory and a surface, by name. Empty for a coherent checkout. */
function inventoryProblems(sources, surfaces) {
  const problems = [];
  for (const version of repeated(sources.modern)) problems.push(`${version} is listed twice in generated/minecraft-versions.json`);
  for (const version of repeated(sources.legacy)) problems.push(`${version} is listed twice in LEGACY_RELEASES`);
  for (const version of sources.modern.filter((release) => sources.legacy.includes(release))) {
    problems.push(`${version} is both legacy and modern`);
  }
  const versions = canonicalReleases(sources);
  const expected = new Map(versions.map((version) => [moduleOf(version), version]));
  const nameSet = (label, names) => {
    for (const name of repeated(names)) problems.push(`${label} repeats ${name}`);
    for (const [name, version] of expected) {
      if (!names.includes(name)) problems.push(`${label} has no ${name} for ${version}`);
    }
    for (const name of names) if (!expected.has(name)) problems.push(`${label} has ${name}, which is no canonical release`);
  };
  nameSet("native/src/versioned/mod.rs modules", surfaces.modules.map((module) => module.name));
  nameSet("SUPPORTED_VERSIONS", surfaces.supported);
  nameSet("rules_for_version", surfaces.profiles);
  nameSet("the tick dispatch", surfaces.dispatch);
  nameSet("native/data/releases", surfaces.dataFiles.map(({ file }) => file.replace(/\.rs$/, "")));
  for (const { file, name, label } of surfaces.modules) {
    const version = expected.get(name);
    if (version && label !== version) problems.push(`${file} declares VERSION ${label}, not ${version}`);
    if (version && file !== `v-${version.replaceAll(".", "_")}.rs`) problems.push(`${name} is read from ${file}`);
  }
  for (const { file, prefix } of surfaces.dataFiles) {
    const version = expected.get(file.replace(/\.rs$/, ""));
    if (version && prefix !== moduleOf(version).toUpperCase()) problems.push(`native/data/releases/${file} holds ${prefix}, not the tables of ${version}`);
  }
  return problems;
}

module.exports = { canonicalReleases, inventoryProblems, inventorySources, moduleOf, readSurfaces };
