// The sha256 of every file generate-all publishes (generated/ and native/data/), raw bytes, sorted by path: the inventory
// two hosts or two runs compare to say their generation is the same to the byte.
//   node scripts/generated-hashes.cjs [--root DIR] [--out FILE]      writes or prints the inventory
//   node scripts/generated-hashes.cjs --compare A B                  lists every path that differs, exit 1 if any
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");

const FOLDERS = ["generated", "native/data"];

function filesUnder(directory, base) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(directory, entry.name);
    return entry.isDirectory() ? filesUnder(file, base) : [path.relative(base, file).replaceAll(path.sep, "/")];
  });
}

/** `path -> sha256` of every generated file under `root`, sorted by code unit so no locale enters the order. */
function inventory(root = path.resolve(__dirname, "..")) {
  const files = FOLDERS.flatMap((folder) => filesUnder(path.join(root, folder), root)).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  return new Map(files.map((file) => [file, crypto.createHash("sha256").update(fs.readFileSync(path.join(root, file))).digest("hex")]));
}

const format = (hashes) => [...hashes].map(([file, hash]) => `${hash}  ${file}\n`).join("");
const parse = (text) => new Map(text.split("\n").filter(Boolean).map((line) => [line.slice(66), line.slice(0, 64)]));

/** The paths whose hash differs or which only one side has. */
function differences(a, b) {
  return [...new Set([...a.keys(), ...b.keys()])].sort().filter((file) => a.get(file) !== b.get(file))
    .map((file) => `${file}: ${a.get(file) ?? "absent"} / ${b.get(file) ?? "absent"}`);
}

if (require.main === module) {
  const option = (name) => {
    const at = process.argv.indexOf(`--${name}`);
    return at < 0 ? null : process.argv[at + 1];
  };
  const compareAt = process.argv.indexOf("--compare");
  if (compareAt >= 0) {
    const [a, b] = process.argv.slice(compareAt + 1, compareAt + 3).map((file) => parse(fs.readFileSync(file, "utf8")));
    const found = differences(a, b);
    for (const line of found) console.log(line);
    console.log(`${a.size} and ${b.size} files, ${found.length} differ`);
    process.exit(found.length > 0 ? 1 : 0);
  }
  const text = format(inventory(option("root") ? path.resolve(option("root")) : undefined));
  if (option("out")) fs.writeFileSync(option("out"), text);
  else process.stdout.write(text);
}

module.exports = { inventory, differences, format, parse };
