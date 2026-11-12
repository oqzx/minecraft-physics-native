// Writes generated/legacy-release-data.json: the block registry, block states, VoxelShape
// provenance (1.13), water/lava fluids and tags of the legacy releases (1.8.8 - 1.13.2), extracted by
// scripts/legacy-release-data.cjs from each release's own decompiled jar under MC_LEGACY_SOURCES
// (scripts/materialize-sources.cjs).
//
// A release whose tree is absent fails the run. With --cached it keeps its previous entry, which is what
// such a run must say it is doing: a fresh run cannot pass on what the sources last proved. A release
// is refused when any of its block properties does not resolve to a source-declared domain.

const fs = require("node:fs");
const path = require("node:path");
const { LEGACY_RELEASES, extractLegacyRelease } = require("./legacy-release-data.cjs");
const { serializePath } = require("./portable-paths.cjs");
const { legacySourcesRoot } = require("./source-roots.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.resolve(
  process.env.MC_LEGACY_RELEASE_MANIFEST ?? path.join(repositoryRoot, "generated", "legacy-release-data.json"),
);
const decompiledRoot = legacySourcesRoot();

// relative to the source root, so a checkout at another depth records the same paths
const relative = (filePath) => serializePath(decompiledRoot, filePath);
const previous = fs.existsSync(outputPath) ? JSON.parse(fs.readFileSync(outputPath, "utf8")).releases ?? {} : {};

const releases = {};
for (const release of LEGACY_RELEASES) {
  const data = extractLegacyRelease(release, decompiledRoot);
  if (!data) {
    if (!process.argv.includes("--cached") || !previous[release.version]) {
      throw new Error(`Missing legacy source tree for ${release.version} (${release.source}) under MC_LEGACY_SOURCES (${decompiledRoot}): run scripts/materialize-sources.cjs`);
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
