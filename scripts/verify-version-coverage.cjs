const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");

const repositoryRoot = path.resolve(__dirname, "..");
const sourceRoot = path.resolve(process.env.MC_DECOMP_SRC ?? path.join(repositoryRoot, "..", "mc-decomp", "src"));
const nativeRoot = path.join(repositoryRoot, "native", "src", "versioned");
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
const requiredFiles = [
  "net/minecraft/world/entity/Entity.java",
  "net/minecraft/world/entity/LivingEntity.java",
  "net/minecraft/world/entity/player/Player.java",
  "net/minecraft/world/phys/shapes/VoxelShape.java",
  "net/minecraft/world/phys/AABB.java",
];
const requiredMethods = {
  "net/minecraft/world/entity/Entity.java": /\bmove\s*\(/,
  "net/minecraft/world/entity/LivingEntity.java": /\btravel\s*\(/,
  "net/minecraft/world/entity/player/Player.java": /\btravel\s*\(/,
};
const expectedLandingRounding = new Map([
  ["1.21.5", "floor"],
  ["1.21.6", "floor"],
  ["1.21.7", "floor"],
  ["1.21.8", "floor"],
  ["1.21.9", "floor"],
  ["1.21.10", "floor"],
  ["1.21.11", "floor"],
  ["26.1", "floor"],
  ["26.1.1", "floor"],
  ["26.1.2", "floor"],
  ["26.2", "floor"],
  ["26.3", "floor"],
]);

function sourceEra(version) {
  const majorMinor = version.split(".").slice(0, 2).join(".");
  return majorMinor === "1.14" ? "1.14" : majorMinor;
}

const versions = [];
const incompleteVersions = [];
for (const era of fs.readdirSync(sourceRoot, { withFileTypes: true })) {
  if (!era.isDirectory() || !/^\d+\.\d+$/.test(era.name)) continue;
  const eraRoot = path.join(sourceRoot, era.name);
  for (const entry of fs.readdirSync(eraRoot, { withFileTypes: true })) {
    if (!entry.isDirectory() || !/^\d+\.\d+(?:\.\d+)?$/.test(entry.name)) continue;
    const version = entry.name;
    const versionRoot = path.join(eraRoot, version);
    const missing = requiredFiles.filter((file) => !fs.existsSync(path.join(versionRoot, file)));
    if (missing.length > 0) {
      incompleteVersions.push({ version, missing });
      continue;
    }
    for (const [file, methodPattern] of Object.entries(requiredMethods)) {
      const source = fs.readFileSync(path.join(versionRoot, file), "utf8");
      if (!methodPattern.test(source)) {
        throw new Error(`${version} is missing a tracked physics method in ${file}`);
      }
    }
    versions.push(version);
  }
}

for (const incomplete of manifest.incompleteVersions ?? []) {
  if (!incompleteVersions.some(({ version }) => version === incomplete.version)) {
    incompleteVersions.push({
      version: incomplete.version,
      missing: incomplete.missingRequired ?? [],
    });
  }
}

if (incompleteVersions.length > 0) {
  const details = incompleteVersions
    .map(({ version, missing }) => `${version}: ${missing.join(", ")}`)
    .join("; ");
  throw new Error(`Incomplete source snapshots block runtime coverage: ${details}`);
}

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
  const blockSourcePath = path.resolve(path.dirname(path.join(repositoryRoot, "generated", "block-registries.json")), blockRegistry.source);
  if (!fs.existsSync(blockSourcePath)) throw new Error(`${version} block registry source is missing`);
  const blockDigest = crypto.createHash("sha256").update(fs.readFileSync(blockSourcePath)).digest("hex");
  if (blockDigest !== blockRegistry.sha256) throw new Error(`${version} block registry fingerprint mismatch`);
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
  const stateRoot = path.resolve(
    path.dirname(path.join(repositoryRoot, "generated", "block-state-definitions.json")),
    stateRelease.sourceRoot,
  );
  if (!fs.existsSync(stateRoot)) throw new Error(`${version} block state source root is missing`);
  const sharedPropertiesPath = path.resolve(
    path.dirname(path.join(repositoryRoot, "generated", "block-state-definitions.json")),
    stateRelease.sharedPropertiesSource,
  );
  if (!fs.existsSync(sharedPropertiesPath)) {
    throw new Error(`${version} shared block property source is missing`);
  }
  const sharedPropertiesDigest = crypto.createHash("sha256")
    .update(fs.readFileSync(sharedPropertiesPath))
    .digest("hex");
  if (sharedPropertiesDigest !== stateRelease.sharedPropertiesSha256) {
    throw new Error(`${version} shared block property fingerprint mismatch`);
  }
  if (stateRelease.sharedPropertyCount <= 0) {
    throw new Error(`${version} has no generated shared block properties`);
  }
  const stateEntries = Object.entries(stateRelease.blocks);
  if (stateRelease.unresolvedPropertyCount !== 0) {
    throw new Error(`${version} has unresolved block property domains`);
  }
  if (stateEntries.length !== stateRelease.blockClassCount) {
    throw new Error(`${version} block state class count is inconsistent`);
  }
  for (const [className, stateDefinition] of stateEntries) {
    if (stateDefinition.unresolvedProperties.length > 0) {
      throw new Error(
        `${version} retains unresolved property domains for ${className}: ${stateDefinition.unresolvedProperties.join(", ")}`,
      );
    }
    const stateSourcePath = path.resolve(
      path.dirname(path.join(repositoryRoot, "generated", "block-state-definitions.json")),
      stateDefinition.source,
    );
    if (!fs.existsSync(stateSourcePath)) {
      throw new Error(`${version} block state source is missing: ${className}`);
    }
    const stateDigest = crypto.createHash("sha256")
      .update(fs.readFileSync(stateSourcePath))
      .digest("hex");
    if (stateDigest !== stateDefinition.sha256) {
      throw new Error(`${version} block state fingerprint mismatch: ${className}`);
    }
    if (stateDefinition.unresolvedProperties.length !== Object.entries(stateDefinition.properties)
      .filter(([, property]) => property.domain.confidence === "unresolved").length) {
      throw new Error(`${version} unresolved property count is inconsistent: ${className}`);
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
  const livingEntity = fs.readFileSync(
    path.join(sourceRoot, sourceEra(version), version, "net/minecraft/world/entity/LivingEntity.java"),
    "utf8",
  );
  const entitySource = fs.readFileSync(
    path.join(sourceRoot, sourceEra(version), version, "net/minecraft/world/entity/Entity.java"),
    "utf8",
  );
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
  const sourceRounding = /return Mth\.(ceil|floor)\([^;]*FALL_DAMAGE_MULTIPLIER/.exec(livingEntity)?.[1]
    ?? /return Mth\.(ceil|floor)\([^;]*\* var2\)/.exec(livingEntity)?.[1]
    ?? /Mth\.(ceil|floor)\([^;]*\* var2\)/.exec(livingEntity)?.[1];
  if (!sourceRounding) throw new Error(`${version} has no detectable fall-damage rounding source`);
  const expectedRounding = expectedLandingRounding.get(version) ?? "ceil";
  if (sourceRounding !== expectedRounding) {
    throw new Error(`${version} source uses ${sourceRounding}, expected ${expectedRounding}`);
  }
  const manifestVersion = manifest.sourceFiles?.[version];
  if (!manifestVersion) throw new Error(`${version} is absent from the generated source manifest`);
  for (const [file, method] of [
    ["Entity.java", "move"],
    ["LivingEntity.java", "travel"],
    ["Player.java", "travel"],
  ]) {
    if (!auditedVersion.files[file]?.methods?.[method]?.length) {
      throw new Error(`${version} physics audit is missing ${method} in ${file}`);
    }
  }
  for (const file of requiredFiles) {
    const sourcePath = path.join(sourceRoot, sourceEra(version), version, file);
    const manifestFile = manifestVersion.files.find((entry) => entry.path === file.replaceAll(path.sep, "/"));
    if (!manifestFile) throw new Error(`${version} manifest is missing ${file}`);
    const digest = crypto.createHash("sha256").update(fs.readFileSync(sourcePath)).digest("hex");
    if (digest !== manifestFile.sha256) throw new Error(`${version} source fingerprint mismatch for ${file}`);
  }
  if (!moduleSource.includes(`const VERSION: &'static str = "${version}";`)) {
    throw new Error(`${version} is absent from its native source-backed registry module`);
  }
}

console.log(`Verified ${versions.length} complete source-backed releases from 1.14 through 26.x.`);
