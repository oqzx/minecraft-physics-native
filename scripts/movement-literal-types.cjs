// generated/movement-literal-types.json: the type rustc gave each float literal of native/src, which no reading of the
// text can say of an unsuffixed one (B046). It reads the compiler's typed tree, so it needs a build and a few minutes:
//   node scripts/movement-literal-types.cjs --work DIR     runs the compiler on a copy in DIR and writes the record
// RUSTC_BOOTSTRAP lets the release's stable compiler print it; nothing it builds is kept. The record is by line and column
// of the run, and it is the sites' `function, literal` that scripts/generate-movement-sites.cjs reads, so an edit that
// moves no literal keeps it good; a literal with no record fails test/generated-inventories.test.cjs.
//
// The record is pinned to what it was typed from: `source`, the sha256 of native/src's Rust with its comments and blank
// lines left out (codeHash), and `rustc`, the compiler's version. test/generated-inventories.test.cjs recomputes the hash,
// so a record older than the code it types (a literal whose context changed its type, at the same function and text) fails
// there, naming this script, before movement-sites takes a type from it.
const childProcess = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");
const readline = require("node:readline");
const crypto = require("node:crypto");
const { scanRust } = require("./movement-sites.cjs");

/** Rust source without its comments and blank lines: what the compiler types, not what is said about it. */
function codeOnly(text) {
  return text.replaceAll("\r\n", "\n")
    .replace(/("(?:\\.|[^"\\])*")|\/\/[^\n]*|\/\*[\s\S]*?\*\//g, (match, string) => string ?? "")
    .split("\n").map((line) => line.trimEnd()).filter((line) => line.trim() !== "").join("\n");
}

/** The pin of a record: the sha256 of every .rs file of `sourceRoot`, by path, as codeOnly leaves it. */
function codeHash(sourceRoot) {
  const walk = (at) => fs.readdirSync(at, { withFileTypes: true }).flatMap((entry) => (entry.isDirectory() ? walk(path.join(at, entry.name)) : entry.name.endsWith(".rs") ? [path.join(at, entry.name)] : []));
  const hash = crypto.createHash("sha256");
  for (const file of walk(sourceRoot).map((file) => path.relative(sourceRoot, file).replaceAll("\\", "/")).sort()) {
    hash.update(file).update("\0").update(codeOnly(fs.readFileSync(path.join(sourceRoot, file), "utf8"))).update("\0");
  }
  return hash.digest("hex");
}

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.join(repositoryRoot, "generated", "movement-literal-types.json");

/** From a stream of the typed tree's lines: `file:line:column` -> type of each float literal. */
async function literalTypes(lines) {
  const types = new Map();
  // (an Expr prints its kind first and its type after: the literal's span, then the next `ty:`)
  let inLiteral = false;
  let at = null;
  for await (const line of lines) {
    const text = line.trimStart();
    if (text.startsWith("kind: Literal")) {
      inLiteral = true;
    } else if (inLiteral && text.startsWith("span: ")) {
      const span = /^span: (.*?):(\d+):(\d+)/.exec(text);
      if (span) at = `${span[1].replaceAll("\\", "/").replace(/^.*?src\//, "")}:${span[2]}:${span[3]}`;
      inLiteral = false;
    } else if (at && text.startsWith("ty: ")) {
      const type = text.slice(4).replace(/,$/, "");
      if (type === "f32" || type === "f64") types.set(at, type);
      at = null;
    }
  }
  return types;
}

/**
 * A copy of the crate whose generated tables are empty: they are 90 MB of names and ids, which the
 * compiler would hold and print whole. Only the types of native/src are read from it.
 */
function typedCopy(native, work) {
  fs.rmSync(work, { recursive: true, force: true });
  fs.cpSync(native, work, { recursive: true, filter: (from) => !/[\\/]artifacts([\\/]|$)/.test(from) });
  for (const directory of ["data/releases"]) {
    for (const entry of fs.readdirSync(path.join(work, directory))) {
      const file = path.join(work, directory, entry);
      const text = fs.readFileSync(file, "utf8").replaceAll("\r\n", "\n");
      fs.writeFileSync(file, text.replace(/^(pub static \w+: &\[[^=\n]*=) [\s\S]*?;$/gm, "$1 &[];"));
    }
  }
}

async function main() {
  const native = path.join(repositoryRoot, "native");
  const at = process.argv.indexOf("--work");
  if (at < 0) throw new Error("usage: node scripts/movement-literal-types.cjs --work DIR (emptied, then a copy of native/)");
  const work = path.resolve(process.argv[at + 1]);
  typedCopy(native, work);
  const child = childProcess.spawn("cargo", ["rustc", "--lib", "--crate-type", "rlib", "--", "-Zunpretty=thir-flat"], {
    cwd: work,
    env: { ...process.env, RUSTC_BOOTSTRAP: "1" },
    stdio: ["ignore", "pipe", "inherit"],
  });
  const types = await literalTypes(readline.createInterface({ input: child.stdout }));
  const code = await new Promise((resolve) => child.on("close", resolve));
  if (code !== 0) throw new Error(`cargo rustc exited with ${code}`);
  const record = {};
  const sourceRoot = path.join(native, "src");
  const walk = (at) => fs.readdirSync(at, { withFileTypes: true }).flatMap((entry) => (entry.isDirectory() ? walk(path.join(at, entry.name)) : entry.name.endsWith(".rs") ? [path.join(at, entry.name)] : []));
  let missing = 0;
  for (const file of walk(sourceRoot)) {
    const name = path.relative(sourceRoot, file).replaceAll("\\", "/");
    const text = fs.readFileSync(file, "utf8").replaceAll("\r\n", "\n");
    // (a whole number is typed `int` by its own row: the record is of floats)
    const sites = scanRust(text).sites.filter(({ whole }) => !whole);
    const source = text.split("\n");
    const seen = new Map();
    for (const site of sites) {
      // (the column of the nth such literal of the line)
      const key = `${site.line}:${site.text}`;
      const nth = seen.get(key) ?? 0;
      seen.set(key, nth + 1);
      let column = -1;
      const text = source[site.line - 1];
      for (let index = 0; index <= nth; index += 1) {
        do column = text.indexOf(site.text, column + 1);
        while (column > 0 && /[\d.]/.test(text[column - 1]));
      }
      const type = types.get(`${name}:${site.line}:${column + 1}`) ?? null;
      if (!type) missing += 1;
      // (in the order of the sites, so the nth site of a function and a literal finds its own)
      const row = (record[name] ??= {});
      (row[`${site.fn}|${site.text}`] ??= []).push(type);
    }
  }
  const sorted = Object.fromEntries(Object.entries(record).sort(([a], [b]) => (a < b ? -1 : 1)).map(([file, rows]) => [file, Object.fromEntries(Object.entries(rows).sort(([a], [b]) => (a < b ? -1 : 1)))]));
  const rustc = childProcess.execFileSync("rustc", ["--version"], { cwd: work, encoding: "utf8" }).trim();
  fs.writeFileSync(outputPath, `${JSON.stringify({ schema: 1, generatedBy: "scripts/movement-literal-types.cjs", rustc, source: codeHash(sourceRoot), literals: sorted }, null, 1)}\n`);
  console.log(`${types.size} typed literals in the tree, ${missing} sites without one (const items and macros have no body of their own)`);
}

if (require.main === module) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}

module.exports = { literalTypes, codeHash, codeOnly };
