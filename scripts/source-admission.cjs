// What makes a release's source record acceptable (B002, docs/source-records.md). A record is held to
// the pins it was made from, so a changed hash, a wrong embedded identity or missing mappings fail
// here with the release named, whatever the manifest says of itself.
const { PINNED_CLIENTS, PINNED_SOURCES, TREE_CLIENTS, NAMED_JARS } = require("./source-pins.cjs");

const SHA1 = /^[0-9a-f]{40}$/;
const SHA256 = /^[0-9a-f]{64}$/;

/** What is wrong with the record of one release: empty when it may be cited. */
function sourceRecordProblems(version, record) {
  const { source, mappings } = record;
  const problems = [];
  const fail = (problem) => problems.push(`${version}: ${problem}`);
  if (source.status === "alias") {
    if (source.embedded !== source.sourceRelease) fail(`the alias serves ${source.sourceRelease} from sources embedding ${source.embedded}`);
    if (!SHA256.test(source.evidence ?? "")) fail("the alias cites no evidence hash");
  } else if (source.status !== "embedded") {
    fail(`the source is ${source.status}${source.embedded ? `, embedding ${source.embedded}` : ""}`);
    return problems;
  } else if (source.embedded !== version) {
    fail(`the source embeds ${source.embedded}`);
  }

  if (mappings?.kind === "mojang" && !SHA1.test(mappings.sha1 ?? "")) fail("Mojang's mappings have no sha1");
  if (mappings?.kind === "unobfuscated" && !/^\d\d\./.test(version)) fail("only 26.x ships unobfuscated");
  // before 1.14.4 Mojang published no mappings: the source is in the recorder's (legacy-mappings/<release>.tiny)
  if (mappings?.kind === "unpublished" && !["recorder", "yarn"].includes(source.mapping)) fail("no mappings are published and the source names none");
  if (!["mojang", "unobfuscated", "unpublished"].includes(mappings?.kind)) fail("the mappings are missing");
  if (mappings?.kind === "mojang" && source.mapping !== "mojang") fail(`the source is in ${source.mapping} names, not the mappings recorded`);

  if (source.status === "alias") return problems;
  const pinned = PINNED_SOURCES[version];
  if (pinned ? source.commit !== pinned : source.commit !== undefined) fail(`the source commit ${source.commit} is not the pinned ${pinned ?? "absence of one"}`);
  const client = PINNED_CLIENTS[version]?.client ?? TREE_CLIENTS[version];
  if (client !== undefined && source.clientJar?.sha1 !== client) fail(`the client jar ${source.clientJar?.sha1} is not the pinned ${client}`);
  const remapped = PINNED_CLIENTS[version]?.remapped ?? (source.mapping === "recorder" ? NAMED_JARS[version] : undefined);
  if (remapped !== undefined && source.clientJar?.remappedSha256 !== remapped) fail(`the remapped jar ${source.clientJar?.remappedSha256} is not the pinned ${remapped}`);
  if (source.tree !== undefined && !SHA256.test(source.tree)) fail("the source tree hash is malformed");
  if (!source.commit && !source.clientJar?.sha1 && !source.tree) fail("no commit, client jar or tree hash identifies the source");
  return problems;
}

/**
 * Whether the record can be reproduced from what it cites. A tree someone decompiled earlier is
 * identified by its hash, but nothing here can make it again: that is a certification blocker.
 */
function sourceBlocker(record) {
  const { source } = record;
  if (source.status !== "embedded" || source.commit || source.decompiler) return null;
  if (source.mapping === "yarn") return `the decompiled tree is pinned by hash; its decompiler and yarn build are not recorded, so it cannot be reproduced`;
  return null;
}

module.exports = { sourceBlocker, sourceRecordProblems };
