// Checks every modern release's generated data, native module and recorded source inputs against
// each other. Inputs are read through what the manifests record (scripts/source-inputs.cjs): a
// mismatch fails, an input not at hand is reported and leaves the release `cached`. --fresh fails on
// any cached release, so a run on a host without the sources cannot pass as source-backed.
const fs = require("node:fs");
const path = require("node:path");
const { nativePolicy, parseSimulation, sourcePolicy } = require("./verify-landing-policy.cjs");
const { readRecorded, sha256 } = require("./source-inputs.cjs");
const { modernTree } = require("./source-roots.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const generatedRoot = path.join(repositoryRoot, "generated");
const fresh = process.argv.includes("--fresh");
const nativeRoot = path.join(repositoryRoot, "native", "src", "versioned");
const simulation = parseSimulation(fs.readFileSync(path.join(repositoryRoot, "native", "src", "simulation.rs"), "utf8"));
const nativeRegistry = fs.readFileSync(
  path.join(nativeRoot, "mod.rs"),
  "utf8",
);
const manifest = JSON.parse(fs.readFileSync(
  path.join(repositoryRoot, "generated", "minecraft-versions.json"),
  "utf8",
));
const methodAudit = JSON.parse(fs.readFileSync(
  path.join(repositoryRoot, "generated", "physics-methods.json"),
  "utf8",
));
const blockRegistries = JSON.parse(fs.readFileSync(
  path.join(repositoryRoot, "generated", "block-registries.json"),
  "utf8",
));
const blockStates = JSON.parse(fs.readFileSync(
  path.join(repositoryRoot, "generated", "block-state-definitions.json"),
  "utf8",
));
const generatedDataRoot = path.join(repositoryRoot, "native", "data");
const generatedReleaseRoot = path.join(generatedDataRoot, "releases");
const rootReleaseFiles = fs.readdirSync(generatedDataRoot)
  .filter((entry) => /^v\d+_\d+(?:_\d+)?\.rs$/.test(entry));
if (rootReleaseFiles.length > 0) {
  throw new Error(`Generated release modules must live under native/data/releases: ${rootReleaseFiles.join(", ")}`);
}
if (!fs.existsSync(generatedReleaseRoot)) {
  throw new Error("Missing native/data/releases generated-data directory");
}
function generatedRustFiles(directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const entryPath = path.join(directory, entry.name);
    return entry.isDirectory()
      ? generatedRustFiles(entryPath)
      : entry.name.endsWith(".rs") ? [entryPath] : [];
  });
}
const generatedNativeData = generatedRustFiles(generatedDataRoot)
  .sort()
  .map((entryPath) => fs.readFileSync(entryPath, "utf8"))
  .join("\n");
if (!generatedNativeData.includes("collision_shape_boxes:")
  || !generatedNativeData.includes("outline_shape_boxes:")
  || !generatedNativeData.includes("interaction_shape_boxes:")) {
  throw new Error("Generated shape-method geometry fields are absent from native data");
}
if (!generatedNativeData.includes("method_shape_provenance")
  || !generatedNativeData.includes("candidate_box_count:")) {
  throw new Error("Generated shape provenance lookup is absent from native data");
}
if (!generatedNativeData.includes("exact_method_shape_boxes")) {
  throw new Error("Generated exact-shape lookup is absent from native data");
}
const requiredFiles = manifest.requiredFiles;
const versions = manifest.versions;

if ((manifest.incompleteVersions ?? []).length > 0) {
  const details = manifest.incompleteVersions
    .map(({ version, missingRequired }) => `${version}: ${(missingRequired ?? []).join(", ")}`)
    .join("; ");
  throw new Error(`Incomplete source snapshots block runtime coverage: ${details}`);
}

/** The recorded inputs of a release that are checked against their hashes. */
function recordedInputs(version, blockRegistry, stateRelease) {
  const inputs = [[blockRegistry.source, blockRegistry.sha256, "block registry"]];
  inputs.push([stateRelease.sharedPropertiesSource, stateRelease.sharedPropertiesSha256, "shared block properties"]);
  for (const [className, block] of Object.entries(stateRelease.blocks)) inputs.push([block.source, block.sha256, `block state of ${className}`]);
  const entry = manifest.sourceFiles[version];
  for (const file of entry?.files ?? []) {
    if (file.present && file.source) inputs.push([file.source, file.sha256, file.path]);
  }
  return inputs;
}

/** A release's Entity or LivingEntity text as the manifest records it, null when not at hand. */
function recordedText(version, file) {
  const entry = manifest.sourceFiles[version]?.files.find((candidate) => candidate.path === file);
  const bytes = entry?.source ? readRecorded(generatedRoot, [entry.source]).get(entry.source) : null;
  if (bytes) return bytes.toString("utf8");
  const tree = modernTree(version);
  const local = tree ? path.join(tree, file) : null;
  return local && fs.existsSync(local) ? fs.readFileSync(local, "utf8") : null;
}

const cached = new Map();
for (const version of versions) {
  const generatedVersion = version.replaceAll(".", "_");
  if (!generatedNativeData.includes(`pub static V${generatedVersion.toUpperCase()}_BLOCKS`)) {
    throw new Error(`${version} is absent from generated Rust block data`);
  }
  if (!generatedNativeData.includes(`pub static V${generatedVersion.toUpperCase()}_STATE_CLASS_COUNT`)) {
    throw new Error(`${version} is absent from generated Rust state data`);
  }
  if (!generatedNativeData.includes(`pub static V${generatedVersion.toUpperCase()}_FLUIDS`)) {
    throw new Error(`${version} is absent from generated Rust fluid data`);
  }
  if (!generatedNativeData.includes(`pub static V${generatedVersion.toUpperCase()}_FLUID_TAGS`)) {
    throw new Error(`${version} is absent from generated Rust fluid-tag data`);
  }
  if (!generatedNativeData.includes(`pub static V${generatedVersion.toUpperCase()}_BLOCK_TAGS`)) {
    throw new Error(`${version} is absent from generated Rust block-tag data`);
  }
  if (!generatedNativeData.includes("state_count:")) {
    throw new Error(`${version} generated Rust block data has no state cardinalities`);
  }
  if (!generatedNativeData.includes("membership_sha256:")) {
    throw new Error(`${version} generated Rust tag data has no membership provenance`);
  }
  if (!generatedNativeData.includes("has_collision_shape_method:")) {
    throw new Error(`${version} generated Rust block data has no shape-method metadata`);
  }
  if (!generatedNativeData.includes("collision_shape_sha256:")) {
    throw new Error(`${version} generated Rust block data has no shape-method fingerprints`);
  }
  if (!generatedNativeData.includes("collision_shape_state_dependent:")) {
    throw new Error(`${version} generated Rust block data has no state-dependent shape metadata`);
  }
  if (!generatedNativeData.includes("flow_sha256:")) {
    throw new Error(`${version} generated Rust fluid data has no method fingerprints`);
  }
  if (!generatedNativeData.includes("property_domains:")) {
    throw new Error(`${version} generated Rust fluid data has no property domains`);
  }
  if (!generatedNativeData.includes("defaults:")) {
    throw new Error(`${version} generated Rust property data has no defaults`);
  }
  if (!generatedNativeData.includes("property_order:")) {
    throw new Error(`${version} generated Rust block data has no property order`);
  }
  if (!generatedNativeData.includes("pub fn state_index")) {
    throw new Error(`${version} generated Rust data has no state index lookup`);
  }
  if (!generatedNativeData.includes("pub fn fluid_state_index")) {
    throw new Error(`${version} generated Rust data has no fluid state index lookup`);
  }
  const blockRegistry = blockRegistries.versions?.[version];
  if (!blockRegistry || !Array.isArray(blockRegistry.blocks) || blockRegistry.blocks.length === 0) {
    throw new Error(`${version} is absent from the generated block registry manifest`);
  }
  if (!blockRegistry.classes || blockRegistry.unresolvedClassCount < 0) {
    throw new Error(`${version} block registry has no source class mapping`);
  }
  const mappedClassNames = Object.keys(blockRegistry.classes);
  for (const blockName of mappedClassNames) {
    if (!blockRegistry.blocks.includes(blockName)) {
      throw new Error(`${version} class mapping is not a registered block: ${blockName}`);
    }
    const mapping = blockRegistry.classes[blockName];
    if (!mapping.initializer || !mapping.field) {
      throw new Error(`${version} class mapping is incomplete: ${blockName}`);
    }
  }
  const stateRelease = blockStates.releases?.[version];
  if (!stateRelease || !stateRelease.blocks || stateRelease.blockClassCount <= 0) {
    throw new Error(`${version} is absent from the generated block state manifest`);
  }
  if (stateRelease.sharedPropertyCount <= 0) {
    throw new Error(`${version} has no generated shared block properties`);
  }
  const stateEntries = Object.entries(stateRelease.blocks);
  const inputs = recordedInputs(version, blockRegistry, stateRelease);
  const read = readRecorded(generatedRoot, inputs.map(([recorded]) => recorded));
  const missing = [];
  for (const [recorded, digest, label] of inputs) {
    const bytes = read.get(recorded);
    if (bytes === null) missing.push(label);
    else if (sha256(bytes) !== digest) throw new Error(`${version} ${label} fingerprint mismatch (${recorded})`);
  }
  if (missing.length > 0) cached.set(version, missing.length);
  if (stateEntries.length !== stateRelease.blockClassCount) {
    throw new Error(`${version} block state class count is inconsistent`);
  }
  for (const [className, stateDefinition] of stateEntries) {
    // a dynamic definition is the one unresolved kind state_index refuses; any other is a defect
    const unknown = stateDefinition.unresolvedProperties.filter((name) => name !== "<dynamic>");
    if (unknown.length > 0) {
      throw new Error(`${version} retains unresolved property domains for ${className}: ${unknown.join(", ")}`);
    }
  }
  const auditedVersion = methodAudit.releases?.[version];
  if (!auditedVersion) throw new Error(`${version} is absent from the physics method audit`);
  if (!auditedVersion.files || !auditedVersion.behaviorFeatures) {
    throw new Error(`${version} physics audit has no structured behavior features`);
  }
  const moduleName = `v-${version.replaceAll(".", "_")}.rs`;
  const modulePath = path.join(nativeRoot, moduleName);
  if (!fs.existsSync(modulePath)) throw new Error(`${version} has no dedicated native module: ${moduleName}`);
  const moduleSource = fs.readFileSync(modulePath, "utf8");
  if (!moduleSource.includes(`const VERSION: &'static str = "${version}";`)) {
    throw new Error(`${version} native module does not declare its exact release`);
  }
  if (!moduleSource.includes("ultra_warm_lava_flow_scale:")) {
    throw new Error(`${version} native module has no owned ultra-warm lava-current policy`);
  }
  const fluidTravelSplit = auditedVersion.behaviorFeatures.fluidTravelSplit;
  const declaredFluidTravelSplit = moduleSource.includes("fluid_travel_split: true");
  if (fluidTravelSplit !== declaredFluidTravelSplit) {
    throw new Error(`${version} fluid-travel handler flag disagrees with source audit`);
  }
  const registryModuleName = `v${version.replaceAll(".", "_")}`;
  if (!nativeRegistry.includes(`pub mod ${registryModuleName};`)) {
    throw new Error(`${version} has no dedicated native registry entry: ${registryModuleName}`);
  }
  if (!moduleSource.includes("fn landing_damage_rounding")) {
    throw new Error(`${version} has no explicit landing-damage policy`);
  }
  const livingEntity = recordedText(version, "net/minecraft/world/entity/LivingEntity.java");
  const entitySource = recordedText(version, "net/minecraft/world/entity/Entity.java");
  if (livingEntity === null || entitySource === null) {
    cached.set(version, (cached.get(version) ?? 0) + 1);
    continue;
  }
  const sourcePushthrough = /\bpush\s*\(\s*Entity\b[\s\S]*?pushthrough/.test(entitySource);
  if (auditedVersion.behaviorFeatures.entityPushthrough !== sourcePushthrough) {
    throw new Error(`${version} entity-pushthrough audit disagrees with Entity.push source`);
  }
  if (fluidTravelSplit && !/\btravelInFluid\s*\(/.test(livingEntity)) {
    throw new Error(`${version} declares the split fluid-travel handler but LivingEntity has no travelInFluid method`);
  }
  if (!fluidTravelSplit && /\btravelInFluid\s*\(/.test(livingEntity)) {
    throw new Error(`${version} source contains travelInFluid but its version profile does not declare the split handler`);
  }
  // the profile's landing policy (formula, rounding, epsilon) as the engine runs it, against the
  // one this release's LivingEntity computes (scripts/verify-landing-policy.cjs)
  const landingSource = sourcePolicy(livingEntity);
  if (!landingSource) throw new Error(`${version} has no detectable fall-damage computation in LivingEntity`);
  const landingNative = nativePolicy(moduleSource, simulation, moduleName);
  if (landingNative.formula !== landingSource.formula || landingNative.rounding !== landingSource.rounding
    || landingNative.epsilon !== landingSource.epsilon) {
    throw new Error(`${version} profile landing policy disagrees with LivingEntity: `
      + `${JSON.stringify(landingNative)} vs ${JSON.stringify(landingSource)}`);
  }
  if (!manifest.sourceFiles?.[version]) throw new Error(`${version} is absent from the generated source manifest`);
  for (const [file, method] of [
    ["Entity.java", "move"],
    ["LivingEntity.java", "travel"],
    ["Player.java", "travel"],
  ]) {
    if (!auditedVersion.files[file]?.methods?.[method]?.length) {
      throw new Error(`${version} physics audit is missing ${method} in ${file}`);
    }
  }
}

const verified = versions.length - cached.size;
console.log(`Verified ${verified} of ${versions.length} source-backed releases from 1.14 through 26.x.`);
// the releases before them, read from their own decompiled jars (MC_LEGACY_SOURCES): each recorded file at its recorded hash
const legacyRoot = require("./source-roots.cjs").legacySourcesRoot();
const legacy = JSON.parse(fs.readFileSync(path.join(generatedRoot, "legacy-release-data.json"), "utf8")).releases;
let legacyVerified = 0;
for (const [version, release] of Object.entries(legacy)) {
  const inputs = [release.registry, ...Object.values(release.blocks)].filter((input) => input.source);
  const absent = inputs.filter((input) => !fs.existsSync(path.join(legacyRoot, input.source)));
  if (absent.length > 0) {
    cached.set(version, absent.length);
    continue;
  }
  for (const input of inputs) {
    const actual = sha256(fs.readFileSync(path.join(legacyRoot, input.source)));
    if (actual !== input.sha256) throw new Error(`${version}: ${input.source} is ${actual}, recorded ${input.sha256}`);
  }
  legacyVerified += 1;
}
console.log(`Verified ${legacyVerified} of ${Object.keys(legacy).length} legacy releases from their recorded hashes.`);
if (cached.size > 0) {
  console.log(`Cached, inputs not at hand: ${[...cached].map(([version, count]) => `${version} (${count})`).join(", ")}`);
  if (fresh) {
    console.error("--fresh: every release must be read from its sources");
    process.exit(1);
  }
}
