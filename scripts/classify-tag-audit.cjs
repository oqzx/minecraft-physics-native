// generated/tag-audit-dispositions.json: what became of the 2026-11-04 audit's 808 version/tag differences. The audit
// compared the old generated tables with a decompiled tree. Each name it reports is judged here by the release's own
// jar, not by the audit's tree: in the default pack the old table lacked it (missing-data); in an experimental pack and
// not the default one (configuration); not a registered block of that release (wrong-identifier); none of those
// (unresolved). `now` is whether the table in native/data equals the jar's default pack today.
//
// The audit lives outside the repository, so this runs by hand: node scripts/classify-tag-audit.cjs AUDIT_FILE
// It is an input captured at a pin: AUDIT holds its sha256, and an audit file with other bytes is refused (after a review,
// `--accept-audit <sha256>` takes it, and AUDIT is updated). Each jar is taken only at the client sha1 its release pins.
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { openZip } = require("./zip-read.cjs");
const { clientJarRoot } = require("./source-pins.cjs");
const { clientPin } = require("./java-oracle/classpath.cjs");

/** The audit this classifies (E:/mc-physics-demo/audit/tag-differences.json on the host that made it), by its bytes. */
const AUDIT = { file: "tag-differences.json", sha256: "35811f160fea22972e18b9f44b912bd51e68cfdb60db41c43fa8f4a09afd50d7" };
const sha = (bytes, algorithm) => crypto.createHash(algorithm).update(bytes).digest("hex");

/** What is wrong with an audit file's bytes against the pin: null when they are the pinned ones, or the accepted ones. */
function auditProblem(bytes, accepted = null) {
  const digest = sha(bytes, "sha256");
  if (digest === AUDIT.sha256 || digest === accepted) return null;
  return `the audit is ${digest}, pinned ${AUDIT.sha256}: review it, then pass --accept-audit ${digest} and update AUDIT`;
}
const { parseTagFile, TAG_FILE, mergePacks, resolveTags } = require("./generate-block-tags.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.join(repositoryRoot, "generated", "tag-audit-dispositions.json");
const generated = (name) => JSON.parse(fs.readFileSync(path.join(repositoryRoot, "generated", name), "utf8"));

/** The default pack's tags and each experimental pack's over it, fully resolved: the audit compared resolved sets. */
function jarTags(version) {
  const jar = path.join(clientJarRoot(), version, `minecraft-${version}-client.jar`);
  const pinned = clientPin(version);
  const actual = sha(fs.readFileSync(jar), "sha1");
  if (!pinned || actual !== pinned) throw new Error(`${version}: ${jar} is ${actual}, pinned ${pinned ?? "nothing"}`);
  const zip = openZip(jar);
  const base = new Map();
  const packs = new Map();
  for (const name of zip.names) {
    const match = TAG_FILE.exec(name);
    if (!match) continue;
    const file = parseTagFile(zip.read(name).toString("utf8"), name);
    if (!match[1]) base.set(match[2], file);
    else {
      if (!packs.has(match[1])) packs.set(match[1], new Map());
      packs.get(match[1]).set(match[2], file);
    }
  }
  return {
    base: resolveTags(mergePacks([base]), null, version),
    packs: new Map([...packs].map(([pack, files]) => [pack, resolveTags(mergePacks([base, files]), null, `${version} ${pack}`)])),
  };
}

function judge(name, id, { base, packs }, known) {
  if (base.get(id)?.has(name)) return "missing-data";
  for (const [pack, tags] of packs) if (tags.get(id)?.has(name)) return `configuration:${pack}`;
  if (known && !known.has(name)) return "wrong-identifier";
  return "unresolved";
}

function main() {
  const auditFile = process.argv[2];
  const bytes = fs.readFileSync(auditFile);
  const accepted = process.argv.includes("--accept-audit") ? process.argv[process.argv.indexOf("--accept-audit") + 1] : null;
  const problem = auditProblem(bytes, accepted);
  if (problem) throw new Error(problem);
  const audit = JSON.parse(bytes.toString("utf8"));
  const registries = generated("block-registries.json").versions;
  const tags = generated("block-tags.json").releases;
  const rows = [];
  const totals = {};
  for (const { version, differences } of audit) {
    if (differences.length === 0) continue;
    const jar = jarTags(version);
    const known = registries[version]?.blocks ? new Set(registries[version].blocks) : null;
    for (const { id, generatedExists, missing, extra } of differences) {
      const missingBy = {};
      const extraBy = {};
      for (const name of missing) {
        const verdict = judge(name, id, jar, known);
        (missingBy[verdict] ??= []).push(name);
      }
      for (const name of extra) {
        // a name the old table held and the default pack has too is the audit tree's difference, not the table's
        const verdict = judge(name, id, jar, known);
        const key = verdict === "missing-data" ? "in-default-pack" : verdict;
        (extraBy[key] ??= []).push(name);
      }
      const kinds = [...new Set([...Object.keys(missingBy), ...Object.keys(extraBy)])].sort();
      const disposition = kinds.length === 0 ? "no-members-reported"
        : kinds.some((kind) => kind === "unresolved") ? "unresolved"
        : kinds.some((kind) => kind.startsWith("configuration")) ? "configuration"
        : kinds.includes("wrong-identifier") ? "wrong-identifier"
        : kinds.includes("in-default-pack") && !kinds.includes("missing-data") ? "audit-tree"
        : "missing-data";
      totals[disposition] = (totals[disposition] ?? 0) + 1;
      const entry = tags[version]?.default?.[id];
      rows.push({
        version, tag: id, generatedExisted: generatedExists, disposition,
        missing: missingBy, extra: extraBy,
        source: entry ? `client ${tags[version].client} default pack, sha256 ${entry.file}`
          : tags[version]?.declaredEmpty?.[id] ? `client ${tags[version].client}: no default file, packs ${tags[version].declaredEmpty[id].join(",") || "none"}`
          : `client ${tags[version]?.client}: no such tag`,
      });
    }
  }
  const output = {
    schema: 1,
    generatedBy: "scripts/classify-tag-audit.cjs",
    audit: { file: path.basename(auditFile), sha256: crypto.createHash("sha256").update(bytes).digest("hex") },
    totals,
    rows,
  };
  fs.writeFileSync(outputPath, `${JSON.stringify(output, null, 1)}\n`);
  console.log(rows.length, totals);
}

if (require.main === module) main();

module.exports = { judge, auditProblem, AUDIT };
