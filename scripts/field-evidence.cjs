// Which sources declare each vanilla member the field contracts cite (B271), and when the member itself, followed by
// identity through renames, came and went. For an anchor `Class.member` it reads the class at every release with a named
// source: the six legacy trees (1.8.8 to 1.13.2, MC_LEGACY_SOURCES, remapped to the names 1.14.4 gives the same member) and
// every Mojang-named client from 1.14.4 (scripts/materialize-sources.cjs). Only a declaration counts, read by structure
// (scripts/java-declarations.cjs): a field, a method, an enum constant, a record component or a nested class of that name.
// A local variable or a call of that name is no evidence. It writes docs/field-evidence.json with, for each anchor:
//   first, last, gaps   the first and last release whose source declares that name, and any release between that does not
//   oldest, at          the file and line of the declaration in the first and in the newest of them
//   since, until        where the member's identity starts and ends (scripts/field-lineage.cjs): `added` and `removed` are
//                       a mechanic vanilla gained or lost there, `oldest` and `newest` reach the end of the sources, and
//                       `unresolved` is a boundary whose identity no mapping can follow
// An anchor no source declares stops the run, so a contract cannot cite what vanilla does not have. It needs the sources and
// the mappings, so it runs by hand; the catalogue reads what it wrote, and test/field-evidence.test.cjs reads each recorded
// line back from the sources where they are present.
//   node scripts/field-evidence.cjs                 writes the record
//   node scripts/field-evidence.cjs --only <file>   checks one contract file and writes nothing
// An anchor is `Class.member`, `ClassA|ClassB.member` for a class that was renamed, `Class.memberA|memberB` for a member that
// was, and `ClassA.memberA|ClassB.memberB` for one mechanic held by either. `Class` alone cites the class.
const fs = require("node:fs");
const path = require("node:path");
const { compareReleases } = require("./registered-releases.cjs");
const { mcSourceRoot, sourceCache } = require("./source-pins.cjs");
const { memberDeclarations, methodDeclarations } = require("./java-declarations.cjs");
const { javaCode } = require("./java-source.cjs");
const lineage = require("./field-lineage.cjs");
const { READ_EDGES, readEdge } = require("./field-read-edges.cjs");

const root = path.resolve(__dirname, "..");
const output = path.join(root, "docs", "field-evidence.json");
const legacyRoot = () => process.env.MC_LEGACY_SOURCES ?? path.join(sourceCache(), "..", "legacy-src");

/** A source: `files()` lists its .java paths, `read(file)` gives a file's text, `base` is the folder paths are relative to. */
function tree(directory, release) {
  let listing;
  const walk = (at) => fs.readdirSync(at, { withFileTypes: true }).flatMap((entry) => (entry.isDirectory() ? walk(path.join(at, entry.name)) : entry.name.endsWith(".java") ? [path.join(at, entry.name)] : []));
  return { release, base: directory, files: () => (listing ??= walk(directory)), read: (file) => fs.readFileSync(file, "utf8") };
}

/** Every named source, oldest first: the legacy trees, the source cache, and 26.2 and 26.3 from MC_SOURCE_ROOT. */
function sources() {
  const trees = new Map();
  if (fs.existsSync(legacyRoot())) {
    for (const release of fs.readdirSync(legacyRoot())) {
      const directory = path.join(legacyRoot(), release, "net", "minecraft");
      if (lineage.isLegacy(release) && fs.existsSync(directory)) trees.set(release, directory);
    }
  }
  for (const base of [sourceCache(), mcSourceRoot()]) {
    if (!fs.existsSync(base)) continue;
    for (const release of fs.readdirSync(base)) {
      const directory = path.join(base, release, "client", "net", "minecraft");
      if (fs.existsSync(directory)) trees.set(release, directory);
    }
  }
  return [...trees.keys()].sort(compareReleases).map((release) => tree(trees.get(release), release));
}

/** The folder a release's recorded paths are relative to, or null when this machine does not have it. */
function sourceBase(release) {
  const candidates = lineage.isLegacy(release) ? [path.join(legacyRoot(), release, "net", "minecraft")]
    : [path.join(sourceCache(), release, "client", "net", "minecraft"), path.join(mcSourceRoot(), release, "client", "net", "minecraft")];
  return candidates.find((directory) => fs.existsSync(directory)) ?? null;
}

/** An anchor as groups of class alternatives, each with its member alternatives. */
function parseAnchor(anchor) {
  const groups = [];
  let classes = [];
  for (const piece of anchor.split("|")) {
    const dot = piece.indexOf(".");
    if (dot !== -1) {
      groups.push({ classes: [...classes, piece.slice(0, dot)], members: [piece.slice(dot + 1).replace(/\(\)$/, "")] });
      classes = [];
    } else if (groups.length > 0) {
      groups.at(-1).members.push(piece.replace(/\(\)$/, ""));
    } else {
      classes.push(piece);
    }
  }
  if (classes.length > 0) groups.push({ classes, members: [] });
  return groups;
}

const ABSTRACT = /^\s*(?:@[\w$.]+(?:\([^)]*\))?\s+)*(?:(?:public|protected|private|static|abstract|default|synchronized|native)\s+)*[\w$<>\[\],.? ]+\s([\w$]+)\s*\([^)]*\)\s*(?:throws [\w$., ]+)?;\s*$/;
const CLASS = /(?:^|[^\w$.])(?:class|enum|interface|record)\s+([A-Za-z_$][\w$]*)(?:\s*<[^>]*>)?(?:\s*\(([^)]*)\))?/g;

/** name -> the first line declaring it in a Java source, any depth of named classes. */
function declarations(text) {
  const lines = new Map();
  const add = (name, line) => { if (!lines.has(name) || lines.get(name) > line) lines.set(name, line); };
  for (const member of memberDeclarations(text)) if (member.kind !== "constructor") add(member.name, member.line);
  for (const method of methodDeclarations(text)) add(method.name, method.line);
  const code = javaCode(text).split("\n");
  code.forEach((line, index) => {
    const abstract = ABSTRACT.exec(line);
    if (abstract && !/^\s*(return|new|throw|else)\b/.test(line)) add(abstract[1], index + 1);
    for (const match of line.matchAll(CLASS)) {
      add(match[1], index + 1);
      for (const component of (match[2] ?? "").split(",")) {
        const name = /([A-Za-z_$][\w$]*)\s*$/.exec(component.trim())?.[1];
        if (name) add(name, index + 1);
      }
    }
  });
  return lines;
}

/** Per source, `${release}:${file}` -> its declarations, read once. */
function reader(cache = new Map()) {
  return (source, file) => {
    const key = `${source.release}:${file}`;
    if (!cache.has(key)) cache.set(key, declarations(source.read(file)));
    return cache.get(key);
  };
}

const filesOf = (source, name) => source.files().filter((file) => file.replaceAll("\\", "/").endsWith(`/${name}.java`));
const relative = (source, file) => path.relative(source.base, file).replaceAll("\\", "/");

/** Where a source declares any member of the anchor's groups: { file, line } or null. */
function declarationIn(source, groups, read) {
  for (const { classes, members } of groups) {
    for (const name of classes) {
      for (const file of filesOf(source, name)) {
        if (members.length === 0) return { file: relative(source, file), line: 1 };
        const declared = read(source, file);
        const line = members.map((member) => declared.get(member)).find(Boolean);
        if (line) return { file: relative(source, file), line };
      }
    }
  }
  return null;
}

const mappings = new Map();

/** The key a registered member is made with (`AIR_DRAG_MODIFIER = register("air_drag_modifier", ...)`), or null. */
function registryKey(source, groups) {
  for (const { classes, members } of groups) {
    for (const name of classes) {
      for (const file of filesOf(source, name)) {
        const text = source.read(file);
        for (const member of members) {
          const made = new RegExp(String.raw`(?:^|[^\w$])${member}\s*=\s*[\w.]+\(\s*"([a-z0-9_./:-]+)"`, "m").exec(text);
          if (made) return made[1];
        }
      }
    }
  }
  return null;
}

const literals = new Map();
/** Whether any source of a release holds a string literal. */
function holdsLiteral(source, literal) {
  const key = `${source.release}:${literal}`;
  if (!literals.has(key)) literals.set(key, source.files().some((file) => source.read(file).includes(`"${literal}"`)));
  return literals.get(key);
}

function record(anchor, list, read) {
  const groups = parseAnchor(anchor);
  const seen = [];
  for (const source of list) {
    const found = declarationIn(source, groups, read);
    if (found) seen.push({ release: source.release, ...found });
  }
  if (seen.length === 0) return null;
  const releases = list.map((source) => source.release);
  const held = new Set(seen.map(({ release }) => release));
  const first = releases.indexOf(seen[0].release);
  const last = releases.indexOf(seen.at(-1).release);
  const byRelease = new Map(list.map((source) => [source.release, source]));
  const context = {
    cache: mappings,
    declares: (wanted, release) => Boolean(declarationIn(byRelease.get(release), wanted, read)),
    classExists: (classes, release) => classes.some((name) => filesOf(byRelease.get(release), name).length > 0),
    registryKey: (wanted, release) => registryKey(byRelease.get(release), wanted),
    holdsLiteral: (literal, release) => holdsLiteral(byRelease.get(release), literal),
  };
  const place = ({ release, file, line }) => `${release} ${file}:${line}`;
  return {
    first: seen[0].release, last: seen.at(-1).release, gaps: releases.slice(first, last + 1).filter((release) => !held.has(release)),
    oldest: place(seen[0]), at: place(seen.at(-1)),
    since: readSince(anchor, lineage.since(groups, seen[0].release, releases, context), releases, byRelease),
    until: lineage.until(groups, seen.at(-1).release, releases, context),
  };
}

/**
 * An unresolved `since` the trees were read for (scripts/field-read-edges.cjs): `added` when the reading holds in the release
 * beside it. A reading of another edge, or one the tree denies, stops the run.
 */
function readSince(anchor, since, releases, byRelease) {
  const entry = READ_EDGES[anchor];
  if (!entry) return since;
  const beside = releases[releases.indexOf(since.release) - 1];
  if (since.by !== "unresolved" || since.release !== entry.release || beside !== entry.beside) {
    throw new Error(`${anchor}: read at ${entry.beside}..${entry.release}, but its since is ${since.release} ${since.by} (beside ${beside})`);
  }
  const { problems, holder } = readEdge(entry, byRelease.get(beside).base, declarations);
  if (problems.length > 0) throw new Error(`${anchor}: ${problems.join("; ")}`);
  return { release: since.release, by: "added", note: `read in ${beside}: ${entry.note}${holder ? ` (${beside} ${holder})` : ""}`, classBeside: true };
}

function anchorsOf() {
  const { allAnchors } = require("./field-contracts.cjs");
  return [...new Set(allAnchors())].sort();
}

if (require.main === module) {
  const only = process.argv.includes("--only") ? process.argv[process.argv.indexOf("--only") + 1] : null;
  const list = sources();
  const read = reader();
  const records = {};
  const missing = [];
  const wanted = only ? [...new Set(Object.values(require(path.resolve(only))).flatMap(({ fields }) => Object.values(fields).map(([, , source]) => source)).filter((source) => !source.startsWith("binding:")))] : anchorsOf();
  for (const anchor of wanted) {
    const made = record(anchor, list, read);
    if (made) records[anchor] = made;
    else missing.push(anchor);
  }
  if (missing.length > 0) {
    console.error(`no source declares:\n  ${missing.join("\n  ")}`);
    process.exitCode = 1;
  }
  const edge = (made) => made.since.by !== "oldest" || made.until.by !== "newest";
  if (only) console.log(Object.entries(records).filter(([, made]) => edge(made)).map(([anchor, made]) => `${anchor} ${made.since.release} ${made.since.by}..${made.until.release} ${made.until.by}`).join("\n"));
  else fs.writeFileSync(output, `${JSON.stringify({ sources: list.map(({ release }) => release), records }, null, 1)}\n`);
  console.log(`${Object.keys(records).length} anchors, ${missing.length} missing, ${list.length} sources`);
}

module.exports = { declarations, parseAnchor, record, sourceBase, sources };
