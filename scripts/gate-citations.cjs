// Whether each release gate of the engine names the source it rests on (B007, B046): a gate is cited when the comments that
// speak for its function (its doc block and those inside it, scripts/movement-sites.cjs functionComments) hold a citation
// with a line, `<release> <Class>.java:<line>`, of the gate's own release or of the registered release on either side of it.
// A citation of another release, or of a class with no line, names no source for that boundary.
//
//   node scripts/gate-citations.cjs           the count, and each gate that is not cited
const fs = require("node:fs");
const path = require("node:path");
const { gateSites } = require("./gate-mutants.cjs");
const { scanRust, functionComments, citationsOf } = require("./movement-sites.cjs");

const sourceRoot = path.join(__dirname, "..", "native", "src");

/** The releases a citation may name for a gate: its boundary and the registered releases either side. */
const sidesOf = (site) => [site.boundary, site.lastBefore, site.firstFrom].filter(Boolean);

/**
 * A gate on the JVM a release declares rests on Mojang's version json, not on a class: `<release> version.json javaVersion`
 * cites it (math.rs declared_java_major, B315).
 */
const versionJsonCitations = (comments) => [...comments.matchAll(/(?<![\w.])((?:1\.\d+|2\d\.\d+)(?:\.\d+)?) version\.json javaVersion\b/g)].map((match) => match[1]);

/** Whether `comments` cite a line of a release that `site` divides. */
function citesGate(comments, site) {
  const sides = sidesOf(site);
  return citationsOf(comments).some((citation) => citation.from !== null && sides.includes(citation.release))
    || versionJsonCitations(comments).some((release) => sides.includes(release));
}

/** Every gate of gate-mutants.cjs, with the function that holds it and whether that function cites it. */
function gateCitations() {
  const scans = new Map();
  return gateSites().map((site) => {
    if (!scans.has(site.file)) scans.set(site.file, scanRust(fs.readFileSync(path.join(sourceRoot, site.file), "utf8").replaceAll("\r\n", "\n")));
    const scan = scans.get(site.file);
    // (the innermost function around the line; a gate outside any, in a const or a static, reads the comments above it)
    const fn = scan.functions.filter((item) => item.line <= site.line && item.end >= site.line).sort((a, b) => b.line - a.line)[0];
    let comments = "";
    if (fn) comments = functionComments(fn, scan.comments);
    else for (let line = site.line - 12; line <= site.line; line += 1) if (scan.comments.has(line)) comments += ` ${scan.comments.get(line)}`;
    return { ...site, holder: fn?.name ?? null, cited: citesGate(comments, site) };
  });
}

if (require.main === module) {
  const gates = gateCitations();
  const uncited = gates.filter((gate) => !gate.cited);
  console.log(`${gates.length} gates, ${gates.length - uncited.length} cited, ${uncited.length} not`);
  for (const gate of uncited) console.log(`  ${gate.file}:${gate.line} ${gate.holder ?? "(none)"} ${gate.boundary} (${sidesOf(gate).join(", ")})`);
}

module.exports = { gateCitations, citesGate, sidesOf };
