// generated/movement-constants.json: every decimal literal the engine and the adapter move things by, looked up in the
// vanilla sources of each release. A literal is `source` if a tree of a registered release holds it, with the first and
// last such release and the Java type it has there; `cited` if vanilla's sources do not hold it as a literal but the code
// beside it names the source it comes from (a derived value, a result of an expression); `jdk` for trig.rs's, which are
// StrictMath's and no source of Minecraft's; `unresolved` if none of those. Whole
// numbers and the exact halves are not constants of anything: they are left out.
//
// A runtime value from the server is no literal, so it is not here: this does not replace an attribute by a default.
const fs = require("node:fs");
const path = require("node:path");
const { registeredReleases } = require("./registered-releases.cjs");
const { LEGACY_RELEASES } = require("./legacy-release-data.cjs");
const { modernTree, legacyTree } = require("./source-roots.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.join(repositoryRoot, "generated", "movement-constants.json");
const SOURCE_ROOTS = ["net/minecraft/world/entity", "net/minecraft/world/phys", "net/minecraft/world/level/block", "net/minecraft/world/level/material",
  "net/minecraft/client/player", "net/minecraft/util", "net/minecraft/world/item", "net/minecraft/item", "net/minecraft/entity", "net/minecraft/block", "net/minecraft/fluid", "net/minecraft/client/network",
  "net/minecraft/util/math", "net/minecraft/util/shape"];
const LITERAL = /(?<![\w.])\d+\.\d+(?:[eE][-+]?\d+)?(?:_?f(?:32|64))?(?![\w.])|(?<![\w.])\d+(?:\.\d+)?[eE][-+]?\d+(?:_?f(?:32|64))?(?![\w.])/g;
const JAVA_LITERAL = /(?<![\w.])\d*\.?\d+(?:[eE][-+]?\d+)?[FfDd]?(?![\w.])/g;

/** The key two literals share if they are the same number at float precision or at double. */
const keysOf = (value) => [String(value), String(Math.fround(value))];
const trivial = (value) => Number.isInteger(value * 4);

function javaFiles(directory, found = []) {
  if (!fs.existsSync(directory)) return found;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) javaFiles(file, found);
    else if (entry.name.endsWith(".java")) found.push(file);
  }
  return found;
}

/** key -> Set of `float` and `double` of a tree's literals. */
function treeLiterals(tree) {
  const literals = new Map();
  const seen = new Set();
  for (const root of SOURCE_ROOTS) {
    for (const file of javaFiles(path.join(tree, root))) {
      if (seen.has(file)) continue;
      seen.add(file);
      const text = fs.readFileSync(file, "utf8").replace(/"(?:[^"\\\n]|\\.)*"/g, "");
      for (const match of text.matchAll(JAVA_LITERAL)) {
        if (!/[.eEfFdD]/.test(match[0])) continue;
        const value = Number(match[0].replace(/[FfDd]$/, ""));
        if (!Number.isFinite(value)) continue;
        const type = /[Ff]$/.test(match[0]) ? "float" : "double";
        for (const key of type === "float" ? [String(Math.fround(value)), String(value)] : keysOf(value)) {
          if (!literals.has(key)) literals.set(key, new Set());
          literals.get(key).add(type);
        }
      }
    }
  }
  return literals;
}

/** A file's code without its tests and comments, and the lines of its comments apart. */
function codeLines(file, native) {
  const text = fs.readFileSync(file, "utf8");
  // (the test module only: one #[cfg(test)] item in the middle of a file is not the end of it)
  const body = native ? text.split(/#\[cfg\(test\)\]\s*mod\s/)[0] : text;
  return body.split(/\r?\n/);
}

const cited = (lines, at) => {
  for (let index = Math.max(0, at - 6); index <= at; index += 1) {
    if (/\.java\b|\bjava\b:?\d*|[A-Z][a-z]+[A-Z]\w*\.\w+|1\.\d+(?:\.\d+)? [A-Z]|2\d\.\d+ [A-Z]/.test(lines[index].split("//")[1] ?? "")) return true;
  }
  return false;
};

function usesOf(root, files, native) {
  const uses = new Map();
  for (const file of files) {
    const lines = codeLines(path.join(root, file), native);
    lines.forEach((line, at) => {
      const code = line.split("//")[0].replace(/"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'/g, "");
      if (/^\s*\*|^\s*\/\*/.test(line)) return;
      for (const match of code.matchAll(LITERAL)) {
        const value = Number(match[0].replace(/_?f(?:32|64)$/, ""));
        if (!Number.isFinite(value) || trivial(value)) continue;
        const key = String(value);
        const entry = uses.get(key) ?? uses.set(key, { value, files: new Set(), sites: 0, cited: false }).get(key);
        entry.files.add(file);
        entry.sites += 1;
        entry.cited ||= cited(lines, at);
      }
    });
  }
  return uses;
}

/** A native module of the engine's: every one but its bindings, its tests and the crate root. */
const moved = (file) => file.endsWith(".rs") && !/^napi|_tests?\.rs$|^tests?\.rs$|^lib\.rs$/.test(file);

function generate() {
  const versions = registeredReleases().map(({ version }) => version);
  const legacy = new Set(LEGACY_RELEASES.map((release) => release.version));
  const trees = [];
  const unavailable = [];
  for (const version of versions) {
    const tree = legacy.has(version) ? legacyTree(version) : modernTree(version);
    if (tree && fs.existsSync(tree)) trees.push([version, treeLiterals(tree)]);
    else unavailable.push(version);
  }
  const nativeRoot = path.join(repositoryRoot, "native", "src");
  const nativeFiles = fs.readdirSync(nativeRoot).filter(moved).sort();
  const adapterRoot = path.join(repositoryRoot, "src", "mineflayer");
  const adapterFiles = fs.readdirSync(adapterRoot).filter((file) => file.endsWith(".ts")).sort();
  const constants = {};
  const merge = (uses, where) => {
    for (const [key, use] of uses) {
      const row = (constants[key] ??= { value: use.value, sites: 0, files: [], cited: false });
      row.sites += use.sites;
      row.files = [...new Set([...row.files, ...[...use.files].map((file) => `${where}/${file}`)])].sort();
      row.cited ||= use.cited;
    }
  };
  merge(usesOf(nativeRoot, nativeFiles, true), "native");
  merge(usesOf(adapterRoot, adapterFiles, false), "adapter");
  for (const row of Object.values(constants)) {
    const keys = keysOf(row.value);
    const holding = trees.filter(([, literals]) => keys.some((key) => literals.has(key)));
    const types = new Set(holding.flatMap(([, literals]) => keys.flatMap((key) => [...(literals.get(key) ?? [])])));
    const onlyTrig = row.files.every((file) => file === "native/trig.rs");
    row.status = holding.length > 0 ? "source" : onlyTrig ? "jdk" : row.cited ? "cited" : "unresolved";
    if (holding.length > 0) {
      row.first = holding[0][0];
      row.last = holding.at(-1)[0];
      row.releases = holding.length;
      row.types = [...types].sort();
    }
    // (how often a number is written is no fact of it: an edit elsewhere must not make this file drift)
    delete row.cited;
    delete row.sites;
  }
  const sorted = Object.fromEntries(Object.entries(constants).sort(([, a], [, b]) => a.value - b.value));
  const totals = {};
  for (const { status } of Object.values(sorted)) totals[status] = (totals[status] ?? 0) + 1;
  return { schema: 1, generatedBy: "scripts/generate-movement-constants.cjs", releases: trees.length, unavailable, totals, constants: sorted };
}

function main() {
  const output = `${JSON.stringify(generate(), null, 1)}\n`;
  if (process.argv.includes("--check")) {
    const present = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, "utf8").replaceAll("\r\n", "\n") : "";
    if (present !== output) throw new Error("generated/movement-constants.json differs from a fresh generation");
    return;
  }
  fs.writeFileSync(`${outputPath}.tmp`, output);
  fs.renameSync(`${outputPath}.tmp`, outputPath);
}

if (require.main === module) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}

module.exports = { keysOf, trivial, cited, moved, treeLiterals, usesOf };
