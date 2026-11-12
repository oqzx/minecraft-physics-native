// generated/movement-sites.json (B046): where generated/movement-constants.json says a number exists in vanilla, this says
// which vanilla line a site of the engine stands for. Each numeric literal of native/src and src/mineflayer, by function
// and field (a line moves, a function stays), with:
//   type     what rustc made of it (generated/movement-literal-types.json)
//   proof    `line`: the citation beside it names a release, a class and lines that hold the number; `method`: a cited
//            class holds it in the method or field the function or field is named for; `class`: a class the function
//            cites holds it; `only`: the classes the module cites hold it in that one member, so there is nowhere else for
//            the site to stand for; `file`: a class the module cites holds it, among others; `value`: only somewhere in
//            the tree; `none`
//   java     the Java type, the release, the file and line, and the expression
//   typing   `same` (an f32 for a float, an f64 for a double), `widened` (an f64 holding a Java float's exact value),
//            `differs` (a Java float written as an f64's decimal, or a double as an f32), unknown without a Java match
//   releases the releases the `if release.at_least` around it leave, and those of them whose tree holds no such number
//   status   `extracted` (proved at a class, typed the same), `module` (a class the module ports holds it at its type,
//            and nothing says it is that site's), `reviewed` (scripts/movement-dispositions.cjs says why),
//            `structural` (0, 1 and 2: identity and doubling, not a constant of anything; a zero stays a value), or `open`
// A release with no tree is unavailable evidence, apart from a release whose tree lacks the number. A value the server sends
// is no literal; the static default standing in for one is listed in `defaults`, each reviewed against the field's owner.
const fs = require("node:fs");
const path = require("node:path");
const { registeredReleases, compareReleases } = require("./registered-releases.cjs");
const { LEGACY_RELEASES } = require("./legacy-release-data.cjs");
const { modernTree } = require("./source-roots.cjs");
const { sourceCache } = require("./source-pins.cjs");
const { scanRust, scanTypeScript, citationsOf, functionComments, valueOf, suffixOf } = require("./movement-sites.cjs");
const { DISPOSITIONS, DEFAULTS, PROFILE_SOURCES } = require("./movement-dispositions.cjs");
const { gateCitations } = require("./gate-citations.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.join(repositoryRoot, "generated", "movement-sites.json");
const typesPath = path.join(repositoryRoot, "generated", "movement-literal-types.json");
// audit/movement-site-reviews.json: by a site's id, the line of vanilla's that a reader tied it to (`at`, which must hold the
// number) or why it is no number of vanilla's (`engine`). A row nothing matches any more shows in `staleSiteReviews`.
const siteReviewsPath = path.join(repositoryRoot, "audit", "movement-site-reviews.json");
const SITE_REVIEWS = fs.existsSync(siteReviewsPath) ? JSON.parse(fs.readFileSync(siteReviewsPath, "utf8")).sites : {};
const legacyRoot = process.env.MC_LEGACY_SOURCES ?? path.join(sourceCache(), "..", "legacy-src");

const JAVA_LITERAL = /(?<![\w.])\d*\.?\d+(?:[eE][-+]?\d+)?[FfDdLl]?(?![\w.])/g;
const STRUCTURAL = new Set([0, 1, 2]);
const PROVED = new Set(["line", "method", "class", "only"]);

function javaFiles(directory, found = new Map()) {
  if (!fs.existsSync(directory)) return found;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) javaFiles(file, found);
    else if (entry.name.endsWith(".java")) {
      const name = entry.name.slice(0, -5);
      if (!found.has(name)) found.set(name, []);
      found.get(name).push(file);
    }
  }
  return found;
}

const treeCache = new Map();
const minecraft = (tree) => path.join(tree, "net", "minecraft");
const indexOf = (tree) => {
  if (!treeCache.has(tree)) treeCache.set(tree, javaFiles(minecraft(tree)));
  return treeCache.get(tree);
};

const literalCache = new Map();
/** Each float literal of a Java file: its line, type, value and the line's text. */
function literalsOf(file) {
  if (literalCache.has(file)) return literalCache.get(file);
  const rows = [];
  let method = null;
  fs.readFileSync(file, "utf8").split(/\r?\n/).forEach((text, index) => {
    const code = text.replace(/"(?:[^"\\]|\\.)*"/g, "");
    // (a member of the class stands at one indent in every decompile of these trees: three spaces or four)
    const member = /^(?: {3}| {4}|\t)(?![ \t}])(?:[\w<>[\],.?@ ]+ )?(\w+)(?:\(| = |;)/.exec(code);
    if (member && !/^\s*(?:return|if|for|while|else|this|super)\b/.test(code)) method = member[1];
    for (const match of code.matchAll(JAVA_LITERAL)) {
      const value = Number(match[0].replace(/[FfDdLl]$/, ""));
      if (!Number.isFinite(value)) continue;
      rows.push({ line: index + 1, type: !/[.eEfFdD]/.test(match[0]) ? "int" : /[Ff]$/.test(match[0]) ? "float" : "double", value, method, text: text.trim().slice(0, 160) });
    }
  });
  literalCache.set(file, rows);
  return rows;
}

/** The same number: at double precision, at float precision, or a float's exact value written out. */
const sameNumber = (java, value, type) => (type === "float" ? Math.fround(java) === Math.fround(value) : java === value);
const holds = (row, value) => sameNumber(row.value, value, row.type) || row.value === value;

/** The tree a citation of `release` is looked up in: the release's exact Mojang-named tree, the one its line was read in. */
function treesOf(release, exact) {
  return exact.has(release) ? [exact.get(release)] : [];
}

const camel = (name) => name.replace(/_(\w)/g, (_, letter) => letter.toUpperCase());

/** The row that best stands for a literal of `type`: one of the same Java type where there is one. */
const pick = (rows, type) => rows.find((row) => (type === "f32" ? row.type === "float" : type === "f64" || type === "number" ? row.type === "double" : type === "int" ? row.type === "int" : false)) ?? rows[0];

/** Where a citation holds a number: { proof, row, file }. `names` are the Java members the site's function or field would be. */
function lookCited(citation, value, exact, names = [], type = null) {
  const releases = citation.release ? [citation.release] : ["26.3", "1.21.11", "1.8.8"];
  let best = null;
  for (const release of releases) {
    for (const tree of treesOf(release, exact)) {
      for (const file of indexOf(tree).get(citation.file) ?? []) {
        const rows = literalsOf(file).filter((row) => holds(row, value));
        if (rows.length === 0) continue;
        if (citation.from !== null) {
          const inside = rows.filter((row) => row.line >= citation.from - 2 && row.line <= citation.to + 2);
          if (inside.length > 0) return { proof: "line", row: pick(inside, type), release, file };
        }
        const named = rows.filter((row) => names.includes(row.method));
        if (named.length > 0 && best?.proof !== "method") best = { proof: "method", row: pick(named, type), release, file };
        best ??= { proof: "class", row: pick(rows, type), release, file };
      }
    }
  }
  return best;
}

/** The one member of the classes a module cites that holds a number at a type, or null when there is none or more than one. */
function onlyMember(citations, value, exact, rust) {
  const members = new Set();
  for (const citation of citations) {
    const releases = citation.release ? [citation.release] : ["26.3", "1.21.11", "1.8.8"];
    for (const release of releases) {
      for (const tree of treesOf(release, exact)) {
        for (const file of indexOf(tree).get(citation.file) ?? []) {
          for (const row of literalsOf(file).filter((item) => holds(item, value))) {
            if (["same", "widened"].includes(typing(rust, value, row.type))) members.add(`${citation.file}#${row.method}`);
          }
        }
      }
    }
  }
  return members.size === 1 ? [...members][0] : null;
}

function typing(rust, literal, java) {
  if (!java) return undefined;
  if (rust === "int") return java === "int" ? "same" : "widened";
  if (rust === "number") return java === "double" ? "same" : Number.isInteger(Math.fround(literal) * 2 ** 20) ? "same" : "differs";
  if (!rust) return undefined;
  if (rust === "f32") return java === "float" ? "same" : "differs";
  if (java === "double") return "same";
  // (a float's own bits written out as an f64 is what its widening gives; a shorter decimal is another number)
  return literal === Math.fround(literal) ? "widened" : "differs";
}

const ranges = (versions, all) => {
  const out = [];
  let start = null;
  let last = null;
  for (const version of all) {
    if (versions.includes(version)) {
      start ??= version;
      last = version;
    } else if (start) {
      out.push(start === last ? start : `${start}..${last}`);
      start = null;
    }
  }
  if (start) out.push(start === last ? start : `${start}..${last}`);
  return out;
};

const walkRust = (root, at = root) => fs.readdirSync(at, { withFileTypes: true }).flatMap((entry) => {
  const file = path.join(at, entry.name);
  if (entry.isDirectory()) return walkRust(root, file);
  return entry.name.endsWith(".rs") && !/_tests?\.rs$|^tests?\.rs$/.test(entry.name) ? [path.relative(root, file).replaceAll("\\", "/")] : [];
});

function moduleSites() {
  const modules = [];
  const nativeRoot = path.join(repositoryRoot, "native", "src");
  for (const file of walkRust(nativeRoot).sort()) {
    const text = fs.readFileSync(path.join(nativeRoot, file), "utf8").replaceAll("\r\n", "\n");
    modules.push({ name: `native/${file}`, key: file, scan: scanRust(text), typescript: false });
  }
  const adapterRoot = path.join(repositoryRoot, "src", "mineflayer");
  for (const file of fs.readdirSync(adapterRoot).filter((name) => name.endsWith(".ts")).sort()) {
    const text = fs.readFileSync(path.join(adapterRoot, file), "utf8").replaceAll("\r\n", "\n");
    modules.push({ name: `adapter/${file}`, key: file, scan: scanTypeScript(text), typescript: true });
  }
  return modules;
}

/** The releases in the gates of a site, or of a profile's own release. */
function applying(gates, all, profile) {
  if (profile) return all.filter((version) => version === profile);
  return all.filter((version) => gates.every(({ at, from }) => (compareReleases(version, at.join(".")) >= 0) === from));
}

/** Each registered release's own tree, and the releases with none. */
function exactTrees(versions = registeredReleases().map(({ version }) => version)) {
  const legacy = new Set(LEGACY_RELEASES.map((release) => release.version));
  const exact = new Map();
  const unavailable = [];
  for (const version of versions) {
    const tree = legacy.has(version) ? path.join(legacyRoot, version) : modernTree(version);
    if (tree && fs.existsSync(minecraft(tree))) exact.set(version, tree);
    else unavailable.push(version);
  }
  return { exact, unavailable, legacy };
}

function generate() {
  const versions = registeredReleases().map(({ version }) => version);
  const { exact, unavailable, legacy } = exactTrees(versions);
  // every number of every tree, for `value` and for the release check
  const held = new Map();
  // (MC_MOVEMENT_CACHE keeps them between runs: the trees do not change, and reading all fifty takes minutes)
  const cacheDirectory = process.env.MC_MOVEMENT_CACHE;
  for (const [version, tree] of exact) {
    const cached = cacheDirectory && path.join(cacheDirectory, `${version}.json`);
    if (cached && fs.existsSync(cached)) {
      held.set(version, new Set(JSON.parse(fs.readFileSync(cached, "utf8"))));
      continue;
    }
    const numbers = new Set();
    for (const files of indexOf(tree).values()) {
      for (const file of files) {
        for (const row of literalsOf(file)) {
          numbers.add(row.value);
          if (row.type === "float") numbers.add(Math.fround(row.value));
        }
      }
    }
    held.set(version, numbers);
    if (cached) {
      fs.mkdirSync(cacheDirectory, { recursive: true });
      fs.writeFileSync(cached, JSON.stringify([...numbers]));
    }
    literalCache.clear();
  }
  const types = fs.existsSync(typesPath) ? JSON.parse(fs.readFileSync(typesPath, "utf8")).literals : {};
  const rows = new Map();
  const profileRows = [];
  for (const { name, key, scan, typescript } of moduleSites()) {
    const profileName = /^native\/versioned\/v-(.+)\.rs$/.exec(name)?.[1].replaceAll("_", ".");
    const moduleComments = [...scan.comments.values()].join(" ");
    const moduleCitations = [...new Map(citationsOf(moduleComments).filter((item) => item.from === null).map((item) => [item.file, item])).values()];
    const cited = new Map();
    const seen = new Map();
    const citationsFor = (site) => {
      const fn = scan.functions[site.fnIndex];
      if (!fn) {
        const near = [];
        for (let line = site.line - 8; line <= site.line; line += 1) if (scan.comments.has(line)) near.push(scan.comments.get(line));
        return citationsOf(near.join(" "));
      }
      if (!cited.has(fn)) cited.set(fn, citationsOf(functionComments(fn, scan.comments)));
      return cited.get(fn);
    };
    for (const site of scan.sites) {
      const value = valueOf(site.text);
      if (!Number.isFinite(value) || (site.whole && profileName)) continue;
      if (profileName && site.field) profileRows.push({ release: profileName, field: site.field, value });
      const scoped = profileName ? `native/versioned/*.rs#${site.fn}` : `${name}#${site.fn}`;
      const occurrence = `${site.fn}|${site.text}`;
      const nth = seen.get(occurrence) ?? 0;
      if (!site.whole) seen.set(occurrence, nth + 1);
      const rust = site.whole ? "int" : typescript ? "number" : suffixOf(site.text) ?? types[key]?.[occurrence]?.[nth] ?? null;
      const type = site.narrowed && rust !== "f32" ? "f32" : rust;
      const id = `${scoped}|${site.field ?? ""}|${site.text}|${type ?? "unknown"}`;
      const row = rows.get(id) ?? { id, file: profileName ? "native/versioned/*.rs" : name, fn: site.fn, field: site.field, literal: site.text, value, type: type ?? "unknown", sites: 0, releases: new Set(), citations: [] };
      row.sites += 1;
      for (const version of applying(site.gates, versions, profileName)) row.releases.add(version);
      for (const citation of citationsFor(site)) row.citations.push(citation);
      row.moduleCitations = moduleCitations;
      rows.set(id, row);
    }
  }
  const out = [];
  const used = new Set(["site review"]);
  const usedSiteReviews = new Set();
  const badSiteReviews = [];
  for (const row of rows.values()) {
    const entry = { id: row.id, file: row.file, fn: row.fn, ...(row.field ? { field: row.field } : {}), literal: row.literal, value: row.value, type: row.type };
    const appliesTo = versions.filter((version) => row.releases.has(version));
    const absent = appliesTo.filter((version) => held.has(version) && !held.get(version).has(row.value));
    const noTree = appliesTo.filter((version) => !held.has(version));
    entry.releases = ranges(appliesTo, versions);
    if (absent.length > 0) entry.absentIn = ranges(absent, versions);
    if (noTree.length > 0) entry.noTree = ranges(noTree, versions);
    if (STRUCTURAL.has(row.value)) {
      entry.status = "structural";
      out.push(entry);
      continue;
    }
    let found = null;
    const names = [camel(row.fn), ...(row.field ? [camel(row.field)] : []), row.fn.replace(/^const /, "")];
    const rank = { line: 3, method: 2, class: 1, file: 0 };
    // (a citation of a release the site does not run in says nothing of its type: the gate around it chose another line)
    const inRange = (citation) => !citation.release || appliesTo.some((version) => version === treeRelease(citation.release) || version.startsWith(`${citation.release}.`) || release2(version) === citation.release);
    const fits = (look) => ["same", "widened"].includes(typing(row.type, row.value, look.row.type));
    const better = (look, was) => !was || (fits(look) && !fits(was)) || (fits(look) === fits(was) && rank[look.proof] > rank[was.proof]);
    for (const citation of row.citations.filter(inRange)) {
      const look = lookCited(citation, row.value, exact, names, row.type);
      if (look && better(look, found)) found = look;
    }
    if (!found || !fits(found) || found.proof === "class") {
      for (const citation of row.moduleCitations) {
        const look = lookCited({ ...citation, from: null, to: null }, row.value, exact, names, row.type);
        if (!look) continue;
        const weak = look.proof === "method" ? look : { ...look, proof: "file" };
        if (better(weak, found)) found = weak;
      }
    }
    if (!found) {
      for (const citation of row.citations.filter((item) => !inRange(item))) {
        const look = lookCited(citation, row.value, exact, names, row.type);
        if (look) {
          found = { ...look, proof: "file" };
          break;
        }
      }
    }
    if (found?.proof === "file" && fits(found) && onlyMember(row.moduleCitations, row.value, exact, row.type)) found = { ...found, proof: "only" };
    const siteReview = SITE_REVIEWS[row.id];
    if (siteReview) usedSiteReviews.add(row.id);
    if (siteReview?.at && !["line", "method"].includes(found?.proof)) {
      const [, release, file, from, to] = /^(\S+) (\w+)\.java:(\d+)(?:-(\d+))?$/.exec(siteReview.at) ?? [];
      const look = file && lookCited({ release, file, from: Number(from), to: Number(to ?? from) }, row.value, exact, names, row.type);
      if (look?.proof === "line" && fits(look)) found = look;
      else badSiteReviews.push(row.id);
    }
    entry.proof = found?.proof ?? (appliesTo.length > absent.length + noTree.length ? "value" : "none");
    if (found) {
      const base = found.file.replaceAll("\\", "/").split("/").pop();
      entry.java = { type: found.row.type, at: `${found.release} ${base}:${found.row.line}`, text: found.row.text };
      entry.typing = typing(row.type, row.value, found.row.type);
    }
    const disposition = siteReview?.engine ? { rule: "site review", why: siteReview.engine } : reviewOf(entry);
    if (disposition && !["extracted"].includes(statusBefore(entry))) {
      used.add(disposition.rule);
      entry.status = "reviewed";
      entry.review = disposition.why;
    } else {
      const proved = PROVED.has(entry.proof);
      const typed = entry.typing === "same" || entry.typing === "widened";
      entry.status = proved && typed ? "extracted" : entry.proof === "file" && typed ? "module" : "open";
    }
    out.push(entry);
  }
  out.sort((a, b) => (a.id < b.id ? -1 : 1));
  const stale = DISPOSITIONS.map(({ rule }) => rule).filter((rule) => !used.has(rule));
  const byRelease = Object.fromEntries(versions.map((version) => [version, { tree: exact.has(version) ? (legacy.has(version) ? "legacy" : "modern") : "unavailable", extracted: 0, module: 0, reviewed: 0, structural: 0, open: 0, notInTree: 0 }]));
  for (const entry of out) {
    for (const version of expand(entry.releases, versions)) byRelease[version][entry.status] += 1;
    for (const version of expand(entry.absentIn ?? [], versions)) byRelease[version].notInTree += 1;
  }
  const totals = {};
  for (const { status } of out) totals[status] = (totals[status] ?? 0) + 1;
  return {
    schema: 1,
    generatedBy: "scripts/generate-movement-sites.cjs",
    trees: exact.size,
    unavailable,
    legacySources: [...legacy],
    totals,
    staleReviews: stale,
    staleSiteReviews: Object.keys(SITE_REVIEWS).filter((id) => !usedSiteReviews.has(id)),
    badSiteReviews,
    byRelease,
    profiles: profileCheck(profileRows, exact, versions),
    defaults: defaultsOf(),
    gates: gatesOf(),
    tables: tablesOf(),
    sites: out,
  };
}

/** The status a site has with no review: a proved site needs none. */
function statusBefore(entry) {
  const proved = PROVED.has(entry.proof);
  return proved && (entry.typing === "same" || entry.typing === "widened") ? "extracted" : "other";
}

/** The review of a site: a rule of scripts/movement-dispositions.cjs, or the sixteenths of an adapter's block box. */
function reviewOf(entry) {
  if (entry.file.startsWith("adapter/") && entry.type !== "int" && Number.isInteger(entry.value * 32) && /Shapes$|^const [A-Z_0-9]+$|collisionBoxes/.test(entry.fn)) {
    return { rule: "sixteenths", why: "derived: a block box in sixteenths, Block.box(n / 16); vanilla writes the n, 6.5 of the chain at 1.16.5 ChainBlock.java:22" };
  }
  const found = DISPOSITIONS.find(({ match }) => match.test(entry.id));
  return found ? { rule: found.rule, why: found.why } : null;
}

/** The releases of a list of ranges. */
function expand(listed, versions) {
  return listed.flatMap((item) => {
    const [from, to = from] = item.split("..");
    return versions.slice(versions.indexOf(from), versions.indexOf(to) + 1);
  });
}

/** Each profile's value of a field held by the classes it comes from, in that release's own tree. */
function profileCheck(rows, exact, versions) {
  const check = {};
  for (const [field, classes] of Object.entries(PROFILE_SOURCES)) {
    const values = new Map();
    for (const row of rows.filter((item) => item.field === field)) {
      if (!values.has(row.value)) values.set(row.value, []);
      values.get(row.value).push(row.release);
    }
    check[field] = [...values].map(([value, releases]) => {
      const missing = releases.filter((release) => {
        const index = indexOf(exact.get(release));
        return !classes.some((name) => (index.get(name) ?? []).some((file) => literalsOf(file).some((item) => holds(item, value))));
      });
      const listed = versions.filter((version) => releases.includes(version));
      return { value, classes, releases: ranges(listed, versions), ...(missing.length > 0 ? { missing: ranges(missing, versions) } : {}) };
    });
  }
  return check;
}

const DEFAULT = /\bunwrap_or\((-?[\d_]+(?:\.\d+)?(?:e-?\d+)?(?:_?f(?:32|64))?)\)|\?\? (-?\d+(?:\.\d+)?)(?![\w.])/g;

/** Every static number that stands in for a value a caller or the server has, and what stands behind it. */
function defaultsOf() {
  const found = [];
  const seen = new Set();
  for (const { name, key, typescript } of moduleSites()) {
    const file = path.join(repositoryRoot, typescript ? "src/mineflayer" : "native/src", key);
    if (!typescript && !/napi/.test(key)) continue;
    fs.readFileSync(file, "utf8").split(/\r?\n/).forEach((line) => {
      const code = line.split("//")[0];
      for (const match of code.matchAll(DEFAULT)) {
        const value = Number((match[1] ?? match[2]).replace(/_?f(?:32|64)$/, "").replaceAll("_", ""));
        const field = /([A-Za-z_]\w*)\s*(?:\.unwrap_or\(|\?\? )[^.]*(?:\.\d+)?[^.]*$/.exec(code.slice(0, match.index + match[0].length))?.[1] ?? "";
        const id = `${name}#${field}=${value}`;
        if (seen.has(id)) continue;
        seen.add(id);
        found.push({ id, value, neutral: value === 0 || value === 1 || value === -1, why: DEFAULTS[id] ?? null });
      }
    });
  }
  return found.sort((a, b) => (a.id < b.id ? -1 : 1));
}

/** Each release gate of the engine's code, by function, and those of its gates whose source it does not cite with a line of a
 * release either side (scripts/gate-citations.cjs). */
function gatesOf() {
  const byFunction = new Map();
  for (const gate of gateCitations()) {
    if (gate.file.startsWith("versioned/")) continue;
    const fn = `native/${gate.file}#${gate.holder ?? "(file)"}`;
    if (!byFunction.has(fn)) byFunction.set(fn, { fn, gates: [], uncited: [] });
    const row = byFunction.get(fn);
    if (!row.gates.includes(gate.boundary)) row.gates.push(gate.boundary);
    if (!gate.cited && !row.uncited.includes(gate.boundary)) row.uncited.push(gate.boundary);
  }
  return [...byFunction.values()].map(({ fn, gates, uncited }) => ({ fn, gates, ...(uncited.length > 0 ? { uncited } : {}) })).sort((a, b) => (a.fn < b.fn ? -1 : 1));
}

/** The tables written by hand: the lookup rules that are no literal of a function. */
function tablesOf() {
  const rows = [];
  for (const { name, scan, typescript } of moduleSites()) {
    if (typescript || name.startsWith("native/versioned/")) continue;
    for (const match of scan.code.matchAll(/^\s*(?:pub(?:\([a-z]+\))? )?(?:const|static) ([A-Z][A-Z0-9_]*): &?\[/gm)) rows.push(`${name}#${match[1]}`);
    const lines = scan.code.split("\n");
    for (const fn of scan.functions) {
      // (the scan blanks a string, so a match on names shows as its arms of "")
      const body = lines.slice(fn.line - 1, fn.end).join("\n");
      if ((body.match(/""(?: \| "")* =>/g) ?? []).length >= 3) rows.push(`${name}#${fn.name}()`);
    }
  }
  return [...new Set(rows)].sort();
}

/** The registered release a tree stands for: 1.8.9's is 1.8.8's (generated/release-aliases.json). */
const treeRelease = (release) => ({ "1.8.9": "1.8.8" })[release] ?? release;

/** The family of a release: 1.21 of 1.21.5, for a citation that says the family. */
const release2 = (version) => version.split(".").slice(0, 2).join(".");

function main() {
  const generated = generate();
  // a release with no tree is a cached run's to record (scripts/generate-all.cjs --cached), never a fresh one's
  if (generated.unavailable.length > 0 && !process.argv.includes("--cached")) {
    throw new Error(`no exact source of ${generated.unavailable.join(", ")}: run scripts/materialize-sources.cjs (MC_SOURCE_CACHE, MC_LEGACY_SOURCES)`);
  }
  const output = `${JSON.stringify(generated, null, 1)}\n`;
  if (process.argv.includes("--check")) {
    const present = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, "utf8").replaceAll("\r\n", "\n") : "";
    if (present !== output) throw new Error("generated/movement-sites.json differs from a fresh generation");
    return;
  }
  fs.writeFileSync(`${outputPath}.tmp`, output);
  fs.renameSync(`${outputPath}.tmp`, outputPath);
}

if (require.main === module) {
  try {
    main();
  } catch (error) {
    console.error(error.stack);
    process.exit(1);
  }
}

module.exports = { typing, ranges, applying, moduleSites, exactTrees, lookCited, treesOf, indexOf, literalsOf };
