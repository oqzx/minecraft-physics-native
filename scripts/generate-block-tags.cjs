// generated/block-tags.json: each release's block tags by the pack that holds them, read from the data packs inside
// its client jar, taken only at the sha1 Mojang's version json gives. The default pack is what the Rust tables hold, so
// it is kept here as hashes that bind them to the file they came from. An experimental pack (data/minecraft/datapacks/*)
// never enters those tables: it keeps its own memberships, and what a server with it enabled sees is `resolveTags` over
// default and pack, adding to a tag unless the file says `replace`. `adds` says what that comes to: the blocks each tag
// gains with the pack enabled, nested tags followed, so a consumer asks that and not its own walk of the files.
//
// A tag id BlockTags declares with no file in the default pack is empty by the data, not absent: `declaredEmpty` lists
// those, and which pack fills one. A release before 1.13 has no tags: it decides by predicates, `tags: false`.
//
// A missing jar, a jar of another sha1, a cycle, an unknown block or a reference to no tag stop the run. With --check
// nothing is written and a difference fails. With --cached a release whose jar is absent keeps its entry.
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { openZip } = require("./zip-read.cjs");
const { clientJarRoot, CLIENT_DATA, PINNED_CLIENTS } = require("./source-pins.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.join(repositoryRoot, "generated", "block-tags.json");
const sha256 = (data) => crypto.createHash("sha256").update(data).digest("hex");
const sha1 = (data) => crypto.createHash("sha1").update(data).digest("hex");
const TAG_FILE = /^(?:data\/minecraft\/datapacks\/([^/]+)\/)?data\/minecraft\/tags\/blocks?\/(.+)\.json$/;
const DEFAULT_PACK = "default";

/** A tag file's values: `{ blocks, includes, replace }`, names without their namespace, sorted and without repeats. */
function parseTagFile(text, label) {
  const parsed = JSON.parse(text);
  if (!Array.isArray(parsed.values)) throw new Error(`${label} has no values`);
  const blocks = new Set();
  const includes = new Set();
  for (const value of parsed.values) {
    const name = typeof value === "string" ? value : value?.id;
    if (typeof name !== "string") throw new Error(`${label} holds an entry that is no name: ${JSON.stringify(value)}`);
    if (name.startsWith("#")) includes.add(name.slice(1).replace(/^minecraft:/, ""));
    else blocks.add(name.replace(/^minecraft:/, ""));
  }
  return { blocks: [...blocks].sort(), includes: [...includes].sort(), replace: parsed.replace === true };
}

/** The packs of a jar: pack -> tag -> the file's values and sha256. */
function jarPacks(zip) {
  const packs = new Map();
  for (const name of zip.names) {
    const match = TAG_FILE.exec(name);
    if (!match) continue;
    const pack = match[1] ?? DEFAULT_PACK;
    if (!packs.has(pack)) packs.set(pack, new Map());
    const bytes = zip.read(name);
    packs.get(pack).set(match[2], { ...parseTagFile(bytes.toString("utf8"), name), sha256: sha256(bytes) });
  }
  return packs;
}

/** Tag -> its own files' values merged in pack order; a `replace` drops what the earlier packs gave. */
function mergePacks(layers) {
  const merged = new Map();
  for (const layer of layers) {
    for (const [id, file] of layer) {
      const earlier = !file.replace ? merged.get(id) : undefined;
      merged.set(id, {
        blocks: [...new Set([...(earlier?.blocks ?? []), ...file.blocks])].sort(),
        includes: [...new Set([...(earlier?.includes ?? []), ...file.includes])].sort(),
      });
    }
  }
  return merged;
}

/**
 * Tag -> every block in it, nested tags followed. A cycle, an include of no tag and a block that is not `known` throw,
 * naming the chain: a tag that lost members quietly is the defect this table exists to rule out.
 */
function resolveTags(tags, known, label) {
  const resolved = new Map();
  const visit = (id, chain) => {
    if (resolved.has(id)) return resolved.get(id);
    if (chain.includes(id)) throw new Error(`${label}: tag cycle ${[...chain, id].join(" -> ")}`);
    const tag = tags.get(id);
    if (!tag) throw new Error(`${label}: ${chain.at(-1) ?? "?"} includes #${id}, which no file defines`);
    const members = new Set(tag.blocks);
    for (const included of tag.includes) for (const block of visit(included, [...chain, id])) members.add(block);
    resolved.set(id, members);
    return members;
  };
  for (const id of tags.keys()) visit(id, []);
  if (known) {
    for (const [id, members] of resolved) {
      for (const block of members) if (!known.has(block)) throw new Error(`${label}: #${id} holds ${block}, which the release does not register`);
    }
  }
  return resolved;
}

/**
 * The sha1 a release's client must have: its pin, else the one in its version json, itself taken only at the sha1
 * Mojang's manifest gives (generated/mojang-releases.json), so a release with no pin still has a chain to Mojang.
 */
function pinnedSha1(version, mojang, root) {
  const pinned = PINNED_CLIENTS[version]?.client ?? CLIENT_DATA[version];
  if (pinned) return pinned;
  const file = path.join(root, version, "version.json");
  if (!fs.existsSync(file)) throw new Error(`No version json of ${version}: ${file}`);
  const bytes = fs.readFileSync(file);
  const expected = mojang.find((release) => release.id === version)?.sha1;
  if (sha1(bytes) !== expected) throw new Error(`The version json of ${version} is ${sha1(bytes)}, Mojang's is ${expected}`);
  return JSON.parse(bytes.toString("utf8")).downloads.client.sha1;
}

const membershipHash = (tag) => sha256(JSON.stringify([tag.blocks, tag.includes]));

/** BlockTags' ids of a release as the Rust table has them: an id with no file there is declared and empty. */
function declaredIds(version) {
  const file = path.join(repositoryRoot, "native", "data", "releases", `v${version.replaceAll(".", "_")}.rs`);
  return [...fs.readFileSync(file, "utf8").matchAll(/GeneratedBlockTag \{ id: "([^"]+)"/g)].map((match) => match[1]);
}

function releaseEntry(version, jar, pin, known) {
  const bytes = fs.readFileSync(jar);
  const digest = sha1(bytes);
  if (pin && digest !== pin) throw new Error(`The client jar of ${version} is ${digest}, pinned ${pin}`);
  const packs = jarPacks(openZip(jar));
  const base = packs.get(DEFAULT_PACK);
  if (!base) return { client: digest, tags: false };
  const resolved = resolveTags(mergePacks([base]), known, `${version} default`);
  const experimental = {};
  const adds = {};
  for (const [pack, files] of [...packs].filter(([name]) => name !== DEFAULT_PACK).sort(([a], [b]) => a.localeCompare(b))) {
    const merged = mergePacks([base, files]);
    const enabled = resolveTags(merged, null, `${version} ${pack}`);
    experimental[pack] = Object.fromEntries([...files].sort(([a], [b]) => a.localeCompare(b)).map(([id, file]) => [id, {
      blocks: file.blocks, includes: file.includes, ...(file.replace ? { replace: true } : {}), sha256: file.sha256,
    }]));
    adds[pack] = Object.fromEntries([...enabled].sort(([a], [b]) => a.localeCompare(b)).flatMap(([id, members]) => {
      const added = [...members].filter((block) => !resolved.get(id)?.has(block)).sort();
      return added.length > 0 ? [[id, added]] : [];
    }));
  }
  const declaredEmpty = {};
  for (const id of declaredIds(version)) {
    if (base.has(id)) continue;
    declaredEmpty[id] = Object.keys(experimental).filter((pack) => experimental[pack][id]);
  }
  return {
    client: digest,
    tags: true,
    default: Object.fromEntries([...base].sort(([a], [b]) => a.localeCompare(b)).map(([id, file]) => [id, {
      file: file.sha256, membership: membershipHash(file), resolved: resolved.get(id).size,
    }])),
    declaredEmpty,
    experimental,
    adds,
  };
}

function generate({ cached = false, root = clientJarRoot() } = {}) {
  const generated = (name) => JSON.parse(fs.readFileSync(path.join(repositoryRoot, "generated", name), "utf8"));
  const manifest = generated("release-manifest.json").releases;
  const versions = (Array.isArray(manifest) ? manifest.map((entry) => entry.release) : Object.keys(manifest));
  const mojang = generated("mojang-releases.json").releases;
  const registries = generated("block-registries.json").versions;
  const previous = fs.existsSync(outputPath) ? JSON.parse(fs.readFileSync(outputPath, "utf8")).releases : {};
  const releases = {};
  for (const version of versions) {
    const jar = path.join(root, version, `minecraft-${version}-client.jar`);
    if (!fs.existsSync(jar)) {
      if (cached && previous[version]) {
        releases[version] = previous[version];
        continue;
      }
      throw new Error(`No client jar of ${version}: ${jar}`);
    }
    const known = registries[version]?.blocks ? new Set(registries[version].blocks) : null;
    releases[version] = releaseEntry(version, jar, pinnedSha1(version, mojang, root), known);
  }
  return { schema: 1, generatedBy: "scripts/generate-block-tags.cjs", releases };
}

function main() {
  const check = process.argv.includes("--check");
  const output = `${JSON.stringify(generate({ cached: process.argv.includes("--cached") }), null, 1)}\n`;
  if (check) {
    const present = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, "utf8").replaceAll("\r\n", "\n") : "";
    if (present !== output) throw new Error("generated/block-tags.json differs from a fresh generation");
    return;
  }
  const staged = `${outputPath}.tmp`;
  fs.writeFileSync(staged, output);
  fs.renameSync(staged, outputPath);
}

if (require.main === module) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}

module.exports = { parseTagFile, mergePacks, resolveTags, membershipHash, TAG_FILE };
