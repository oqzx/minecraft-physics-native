// Writes generated/legacy-release-data.json: the block registry, block states, VoxelShape
// provenance (1.13), water/lava fluids and tags of the releases before the mc-decomp trees
// (1.8.8 - 1.13.2), extracted by scripts/legacy-release-data.cjs from the Yarn-named decompiled
// trees under MC_DECOMPILED_ROOT (default ../../pr/mc-decompiled from the package).
//
// A release whose tree is absent keeps its previous entry, so the manifest (and the Rust data
// generated from it) stays as the sources last proved it. A release is refused when any of its
// block properties does not resolve to a source-declared domain.

const fs = require("node:fs");
const path = require("node:path");
const { LEGACY_RELEASES, extractLegacyRelease } = require("./legacy-release-data.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.resolve(
  process.env.MC_LEGACY_RELEASE_MANIFEST ?? path.join(repositoryRoot, "generated", "legacy-release-data.json"),
);
const decompiledRoot = process.env.MC_DECOMPILED_ROOT
  ?? path.join(repositoryRoot, "..", "..", "pr", "mc-decompiled");

const relative = (filePath) => path.relative(path.dirname(outputPath), filePath).replaceAll(path.sep, "/");
const previous = fs.existsSync(outputPath) ? JSON.parse(fs.readFileSync(outputPath, "utf8")).releases ?? {} : {};

const releases = {};
for (const release of LEGACY_RELEASES) {
  const data = extractLegacyRelease(release, decompiledRoot);
  if (!data) {
    if (!previous[release.version]) {
      throw new Error(`Missing legacy source tree for ${release.version} (${release.source}) under ${decompiledRoot}`);
    }
    releases[release.version] = previous[release.version];
    continue;
  }
  if (data.unresolved.length > 0) {
    throw new Error(`Unresolved block properties in ${release.version}:\n  ${data.unresolved.join("\n  ")}`);
  }
  releases[release.version] = {
    sourceRelease: data.sourceRelease,
    standIn: data.standIn,
    flattened: data.flattened,
    registry: {
      source: relative(data.registry.source),
      sha256: data.registry.sha256,
      blocks: Object.keys(data.registry.classes),
      classes: data.registry.classes,
    },
    blocks: Object.fromEntries(Object.entries(data.blocks).map(([className, block]) => [className, {
      source: block.file && relative(block.file),
      sha256: block.sha256,
      properties: block.properties,
      propertyOrder: block.propertyOrder,
      stateCount: block.stateCount,
      shapes: block.shapes,
    }])),
    blockClassCount: Object.keys(data.blocks).length,
    fluids: data.fluids,
    fluidTags: data.fluidTags,
    blockTags: data.blockTags,
  };
}

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, `${JSON.stringify({ schema: 1, releases }, null, 2)}\n`);
