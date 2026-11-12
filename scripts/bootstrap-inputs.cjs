// What a clean host needs before it may regenerate anything, checked before anything is generated:
// every release's exact source, under the name it embeds, and the pinned minecraft-data beside the
// checkout (pnpm-workspace.yaml links ../minecraft-data).
//   node scripts/bootstrap-inputs.cjs            clones minecraft-data at its pin when absent, then checks
//   node scripts/bootstrap-inputs.cjs --check    only checks
//   --cached                                     leaves out the jars, for a --data cached build
// The locations are explicit: MC_SOURCE_ROOT, MC_SOURCE_CACHE, MC_LEGACY_SOURCES,
// MC_EXTRACTED_DATA_REPO, MC_DATA_REPO (docs/release-build.md). A release that cannot be proved is an
// error naming it; a neighbouring release never stands in.
const fs = require("node:fs");
const path = require("node:path");
const { execFileSync, spawnSync } = require("node:child_process");
const { directoryIdentity } = require("./source-identity.cjs");
const { LEGACY_RELEASES } = require("./legacy-release-data.cjs");
const { MINECRAFT_DATA, CLIENT_DATA, extractedRepo, mcSourceRoot, sourceCache } = require("./source-pins.cjs");
const { treeProblem, readPins } = require("./tree-pins.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const dataCheckout = () => process.env.MC_MINECRAFT_DATA ?? path.join(repositoryRoot, "..", "minecraft-data");
const legacySourcesRoot = () => process.env.MC_LEGACY_SOURCES ?? path.join(sourceCache(), "..", "legacy-src");

const head = (directory) => spawnSync("git", ["-C", directory, "rev-parse", "HEAD"], { encoding: "utf8" }).stdout?.trim();

/** The index minecraft-data's own `prepare` writes, which its commit does not hold. */
function buildMinecraftData(target = dataCheckout()) {
  execFileSync(process.execPath, [path.join(target, "bin", "generate_data.js")], { cwd: target, stdio: "inherit" });
}

/** minecraft-data at its pin, submodule included and built, cloned beside the checkout. */
function cloneMinecraftData(target = dataCheckout()) {
  execFileSync("git", ["clone", "--no-checkout", process.env.MC_MINECRAFT_DATA_SOURCE ?? MINECRAFT_DATA.repository, target], { stdio: "inherit" });
  execFileSync("git", ["-C", target, "checkout", "--detach", MINECRAFT_DATA.commit], { stdio: "inherit" });
  execFileSync("git", ["-C", target, "submodule", "init"], { stdio: "inherit" });
  const submodule = process.env.MC_MINECRAFT_DATA_SUBMODULE_SOURCE;
  if (submodule) execFileSync("git", ["-C", target, "config", "submodule.minecraft-data.url", submodule], { stdio: "inherit" });
  execFileSync("git", ["-C", target, "-c", "protocol.file.allow=always", "submodule", "update", ...(submodule ? [] : ["--depth", "1"])], { stdio: "inherit" });
  buildMinecraftData(target);
}

/** Everything in the way of a source-backed run: each entry names a release or an input. */
function inputProblems({ modern = null, data = dataCheckout(), repository = extractedRepo(), pin = MINECRAFT_DATA.commit, dataPin = MINECRAFT_DATA.data, jars = null, pins = readPins(), decompiles = LEGACY_RELEASES.map(({ version }) => version), legacySources = legacySourcesRoot() } = {}) {
  const { modernTree, manifest } = require("./source-roots.cjs");
  const problems = [];
  // a tree that is the right release is still held to its bytes (scripts/source-tree-pins.json)
  const pinned = (key, tree) => {
    const problem = treeProblem(key, { pins, tree });
    if (problem) problems.push(problem);
  };
  for (const version of modern ?? manifest.versions) {
    const tree = modernTree(version);
    if (!tree) {
      problems.push(`${version}: no exact source. Set MC_SOURCE_ROOT or MC_SOURCE_CACHE, or run scripts/materialize-sources.cjs ${version}`);
      continue;
    }
    const relative = path.relative(mcSourceRoot(), tree);
    const fromRoot = !relative.startsWith("..") && !path.isAbsolute(relative);
    pinned(fromRoot ? `mc-source:${version}` : `cache:${version}`, tree);
    // the default data pack of a release whose sources are elsewhere
    if (fromRoot && CLIENT_DATA[version]) pinned(`cache:${version}`, path.join(sourceCache(), version, "client"));
  }
  // the legacy releases' own jars through the pinned Vineflower (scripts/materialize-sources.cjs), which
  // generate-movement-sites and generate-primitive-map read, each the release whose name it embeds
  for (const version of decompiles) {
    const tree = path.join(legacySources, version);
    if (!fs.existsSync(path.join(tree, "net", "minecraft"))) {
      problems.push(`${version}: no decompiled legacy tree under MC_LEGACY_SOURCES (${legacySources}): run scripts/materialize-sources.cjs ${version}`);
      continue;
    }
    const embedded = directoryIdentity(tree);
    if (embedded !== version) problems.push(`${version}: the tree under MC_LEGACY_SOURCES embeds ${embedded}, a wrong-release source`);
    else pinned(`legacy:${version}`, tree);
  }
  // what generate-block-shapes, generate-entity-data and generate-legacy-shapes run: each release's own code, its
  // version json the one Mojang's list pinned, and every library that json names
  const { completeClasspath } = require("./java-oracle/classpath.cjs");
  const { versionJsonPins } = require("./fetch-jar-inputs.cjs");
  const jsonPins = versionJsonPins();
  for (const version of jars ?? [...decompiles, ...manifest.versions]) {
    try {
      const { versionJson } = completeClasspath(version);
      if (versionJson !== jsonPins.get(version)) throw new Error(`its version json is ${versionJson}, pinned ${jsonPins.get(version)}`);
    } catch (error) {
      problems.push(`${version}: no pinned jar to run: ${error.message}. Run scripts/fetch-jar-inputs.cjs with MC_CLIENT_JAR_ROOT and MC_LIBRARY_ROOT set, or build with --data cached, which says so`);
    }
  }
  if (!fs.existsSync(repository)) problems.push(`no extracted data repository at ${repository}: set MC_EXTRACTED_DATA_REPO`);
  if (!fs.existsSync(path.join(data, "package.json"))) problems.push(`no minecraft-data at ${data}: run scripts/bootstrap-inputs.cjs`);
  else if (head(data) !== pin) problems.push(`minecraft-data at ${data} is at ${head(data)}, pinned ${pin}`);
  else if (!fs.existsSync(path.join(data, "data.js"))) problems.push(`minecraft-data at ${data} is not built: run scripts/bootstrap-inputs.cjs`);
  else if (head(path.join(data, "minecraft-data")) !== dataPin) problems.push(`minecraft-data's data at ${data} is at ${head(path.join(data, "minecraft-data")) || "nothing"}, pinned ${dataPin}`);
  return problems;
}

if (require.main === module) {
  if (!process.argv.includes("--check")) {
    if (!fs.existsSync(dataCheckout())) cloneMinecraftData();
    else if (!fs.existsSync(path.join(dataCheckout(), "data.js"))) buildMinecraftData();
  }
  const problems = inputProblems(process.argv.includes("--cached") ? { jars: [] } : {});
  for (const problem of problems) console.error(problem);
  if (problems.length > 0) process.exit(1);
  console.log("inputs: every release has its exact source, minecraft-data is at its pin");
}

module.exports = { inputProblems, cloneMinecraftData };
