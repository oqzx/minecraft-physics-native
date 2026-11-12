// The content of every source tree a generator reads, pinned by hash (scripts/source-tree-pins.json), so a tree is
// held to its bytes and not only to the release it names. Three kinds:
//   cache:<release>       MC_SOURCE_CACHE/<release>/client, made by scripts/materialize-sources.cjs from a pinned
//                         commit or a pinned jar through the pinned Vineflower; the pin says that step reproduced it
//   mc-source:<release>   MC_SOURCE_ROOT/<release>/client (26.2, 26.3), made the same way from a pinned commit
//   legacy:<release>      MC_LEGACY_SOURCES/<release>, the six legacy releases' pinned Mojang-named jars through Vineflower
// A tree's hash covers what generators read: version.json, net/ and data/minecraft/tags/, by relative path, line
// endings aside, so a checkout's line-ending setting does not change it.
//   node scripts/tree-pins.cjs --check [key...]   hashes the trees at hand against their pins
//   node scripts/tree-pins.cjs --write [key...]   records the hashes of the trees at hand (a reviewed change of input)
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");

const pinsPath = path.join(__dirname, "source-tree-pins.json");
const COVERED = ["version.json", "net", path.join("data", "minecraft", "tags")];

/** The files of `directory`, `/`-separated relative paths, in byte order. */
function filesUnder(root, relative = "") {
  const directory = path.join(root, relative);
  if (!fs.existsSync(directory)) return [];
  if (!fs.statSync(directory).isDirectory()) return [relative.replaceAll("\\", "/")];
  return fs.readdirSync(directory).flatMap((name) => filesUnder(root, relative ? path.join(relative, name) : name));
}

/** `{ sha256, files }` of what generators read in a tree, line endings aside. */
function treeHash(tree) {
  const names = COVERED.flatMap((part) => filesUnder(tree, part)).sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
  const hash = crypto.createHash("sha256");
  for (const name of names) {
    const bytes = fs.readFileSync(path.join(tree, name));
    const text = bytes.includes(13) ? Buffer.from(bytes.toString("latin1").replaceAll("\r\n", "\n"), "latin1") : bytes;
    hash.update(name).update("\0").update(String(text.length)).update("\0").update(text);
  }
  return { sha256: hash.digest("hex"), files: names.length };
}

const readPins = (file = pinsPath) => JSON.parse(fs.readFileSync(file, "utf8")).trees;

/** Where the tree of a key is on this host. */
function treeOf(key) {
  const { sourceCache, mcSourceRoot } = require("./source-pins.cjs");
  const [kind, name] = key.split(/:(.*)/s);
  if (kind === "cache") return path.join(sourceCache(), name, "client");
  if (kind === "mc-source") return path.join(mcSourceRoot(), name, "client");
  if (kind === "legacy") return path.join(process.env.MC_LEGACY_SOURCES ?? path.join(sourceCache(), "..", "legacy-src"), name);
  throw new Error(`${key}: not a tree pin`);
}

/** What is wrong with a tree against its pin, null when it holds; `tree` defaults to where the key is on this host. */
function treeProblem(key, { pins = readPins(), tree = treeOf(key) } = {}) {
  const pin = pins[key];
  if (!pin) return `${key}: no pin in scripts/source-tree-pins.json`;
  if (!fs.existsSync(tree)) return `${key}: no tree at ${tree}`;
  const { sha256, files } = treeHash(tree);
  if (sha256 !== pin.sha256) {
    return `${key}: the tree at ${tree} is ${sha256} (${files} files), pinned ${pin.sha256} (${pin.files} files): it is not the pinned input. Remove it and run scripts/materialize-sources.cjs, or supply the pinned tree`;
  }
  return null;
}

if (require.main === module) {
  const write = process.argv.includes("--write");
  const keys = process.argv.slice(2).filter((argument) => !argument.startsWith("--"));
  const document = JSON.parse(fs.readFileSync(pinsPath, "utf8"));
  const wanted = keys.length > 0 ? keys : Object.keys(document.trees);
  let failed = 0;
  for (const key of wanted) {
    if (write) {
      document.trees[key] = treeHash(treeOf(key));
      console.log(`${key} ${document.trees[key].sha256}`);
      continue;
    }
    const problem = treeProblem(key, { pins: document.trees });
    if (problem) {
      failed++;
      console.error(problem);
    } else console.log(`${key} ok`);
  }
  if (write) {
    document.trees = Object.fromEntries(Object.entries(document.trees).sort(([left], [right]) => (left < right ? -1 : 1)));
    fs.writeFileSync(pinsPath, `${JSON.stringify(document, null, 1)}\n`);
  }
  if (failed > 0) process.exit(1);
}

module.exports = { treeHash, treeProblem, treeOf, readPins, pinsPath };
