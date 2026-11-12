// What the generated data can and cannot say of each release, kept in three parts: is its structure whole, where did it
// come from, and has its behaviour been shown. The last is not this report's to answer: a table can be whole and exact
// and the engine still differ, so no release comes out verified here. Every release not fully covered is named.
//   node scripts/report-generated-integrity.cjs [OUTPUT_FILE]
const fs = require("node:fs");
const path = require("node:path");

const repositoryRoot = path.resolve(__dirname, "..");
const generated = (name) => JSON.parse(fs.readFileSync(path.join(repositoryRoot, "generated", name), "utf8"));

function integrityReport() {
  const manifest = generated("release-manifest.json").releases;
  const versions = Object.keys(manifest);
  const runtime = generated("runtime-registries.json").releases;
  const registries = generated("block-registries.json").versions;
  const tags = generated("block-tags.json").releases;
  const shapes = generated("block-shapes.json").releases;
  const projectiles = generated("projectile-capabilities.json").releases;
  const audit = generated("tag-audit-dispositions.json");
  const named = (test) => versions.filter(test);
  const legacyShapes = generated("block-shapes-legacy.json").releases;
  const noShapes = named((version) => shapes[version]?.status !== "exact");
  const alone = noShapes.filter((version) => legacyShapes[version]?.status === "alone");
  const noClientDump = named((version) => !runtime[version]);
  const noTags = named((version) => !tags[version]?.tags);
  const noProjectileSource = named((version) => Object.values(projectiles[version] ?? {}).some(({ status }) => status === "unavailable-source"));
  const unresolved = [
    { what: "no state-by-state geometry from the release's own code", releases: noShapes.filter((version) => !alone.includes(version)) },
    { what: "geometry of a block alone: its neighbours are for the recordings", releases: alone },
    { what: "no running client's block list to hold the tables to", releases: noClientDump },
    { what: "projectile source not at hand", releases: noProjectileSource },
  ].filter(({ releases }) => releases.length > 0);
  return {
    releases: versions.length,
    structure: {
      registries: { checked: Object.keys(registries).length, againstClient: Object.keys(runtime).length },
      states: { total: Object.values(shapes).reduce((sum, release) => sum + (release.states ?? 0), 0) },
      tags: {
        scoped: versions.length - noTags.length,
        predicatesBeforeTags: noTags,
        auditDispositions: audit.totals,
      },
    },
    provenance: {
      sources: Object.fromEntries(Object.entries(Object.groupBy(versions, (version) => manifest[version].source.status)).map(([status, list]) => [status, list.length])),
      shapes: { exact: named((version) => shapes[version]?.status === "exact"), alone, unavailable: noShapes.filter((version) => !alone.includes(version)) },
    },
    behaviour: { status: "not-judged-here", verified: [], evidence: "the recorder's replay against vanilla recordings" },
    unresolved,
  };
}

if (require.main === module) {
  const text = `${JSON.stringify(integrityReport(), null, 2)}\n`;
  if (process.argv[2]) fs.writeFileSync(process.argv[2], text);
  else process.stdout.write(text);
}

module.exports = { integrityReport };
