// The per-release status page (B268, docs/release-status.md), computed from the registry and from what each dimension's own
// file holds. Nothing here is typed in by hand but the ticket links, so a release added or a status changed fails
// test/release-status-doc.test.cjs until this runs again. What the page says of pins is what generated/conformance-evidence.json
// holds and which engine each names: whether a pin is still current is only ever read from scripts/release-admission.cjs.
//   node scripts/release-status-doc.cjs           writes docs/release-status.md
//   node scripts/release-status-doc.cjs --check   fails on a difference
const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");
const { INTERACTIONS, available, engineDigest } = require("./parity-contract.cjs");
const { registeredReleases } = require("./registered-releases.cjs");

const root = path.resolve(__dirname, "..");
const page = path.join(root, "docs", "release-status.md");
const json = (file) => JSON.parse(fs.readFileSync(path.join(root, file), "utf8"));

/** The rows of the pass table of docs/recorder-coverage.md, one for each release it names. */
function passRows(text, releases) {
  const rows = new Map();
  const named = (spec) => {
    const range = /^(\S+) to (\S+)$/.exec(spec);
    if (!range) return spec.split(", ");
    const first = releases.indexOf(range[1]);
    const last = releases.indexOf(range[2]);
    if (first < 0 || last < first) throw new Error(`docs/recorder-coverage.md: no such range: ${spec}`);
    return releases.slice(first, last + 1);
  };
  for (const [, spec, scenarios, notOk, where] of text.matchAll(/^\| ([\d.]+(?:(?:, | to )[\d.]+)*) \| ([\d,]+) \| ([^|]+?) \| (remote|here) \|$/gm)) {
    for (const release of named(spec)) {
      if (rows.has(release)) throw new Error(`docs/recorder-coverage.md: ${release} is in two rows`);
      rows.set(release, { scenarios, notOk, where });
    }
  }
  for (const release of releases) if (!rows.has(release)) throw new Error(`docs/recorder-coverage.md: no row for ${release}`);
  return rows;
}

function sourceCell(status, record) {
  const { source } = record;
  const identity = source.commit ? `commit \`${source.commit.slice(0, 8)}\``
    : source.tree ? `tree \`${source.tree.slice(0, 8)}\``
      : source.clientJar ? `jar \`${source.clientJar.sha1.slice(0, 8)}\`, ${source.decompiler ?? "no decompiler"}`
        : "";
  if (status.status === "alias") return `alias of ${source.sourceRelease}`;
  return `${status.status}, ${identity}`;
}

/** What keeps a release from a claim, worst first. A release is in one state. */
function stateOf(status, pass) {
  if (status.source.status === "blocked" || status.source.status === "rejected" || status.source.status === "missing") return status.source.status === "blocked" ? "blocked: source" : `blocked: source ${status.source.status}`;
  if (status.data.status !== "complete" || status.implementation.status !== "complete") return "incomplete";
  if (status.admitted) return "admitted";
  return pass.notOk === "none" ? "unpinned: no conformance pin" : "unpinned: no conformance pin, recordings short";
}

function collect() {
  const releases = registeredReleases().map(({ version }) => version);
  return {
    releases,
    status: json("src/release-status.json").releases,
    manifest: json("generated/release-manifest.json").releases,
    pins: json("generated/conformance-evidence.json").releases,
    pass: passRows(fs.readFileSync(path.join(root, "docs", "recorder-coverage.md"), "utf8").replace(/\r\n/g, "\n"), releases),
  };
}

/** Consecutive releases with the same value, as a range. */
function grouped(releases, valueOf) {
  const groups = [];
  for (const release of releases) {
    const value = valueOf(release);
    if (groups.at(-1)?.value === value) groups.at(-1).last = release;
    else groups.push({ first: release, last: release, value });
  }
  return groups.map(({ first, last, value }) => [first === last ? first : `${first} to ${last}`, value]);
}

function render(data, generatedLine) {
  const { releases, status, manifest, pins, pass } = data;
  const required = (release) => INTERACTIONS.filter((interaction) => available(interaction, release));
  const pinCount = (release) => Object.keys(pins[release] ?? {}).length;
  const engines = [...new Set(Object.values(pins).flatMap((cells) => Object.values(cells).map((pin) => pin.engine.slice(0, 8))))];
  const commits = [...new Set(Object.values(pins).flatMap((cells) => Object.values(cells).map((pin) => pin.commit.slice(0, 7))))];
  const admitted = releases.filter((release) => status[release].admitted).length;
  const pinned = Object.values(pins).reduce((sum, cells) => sum + Object.keys(cells).length, 0);
  const states = {};
  const rows = releases.map((release) => {
    const entry = status[release];
    const state = stateOf(entry, pass.get(release));
    states[state] = (states[state] ?? 0) + 1;
    const { scenarios, notOk } = pass.get(release);
    return `| ${release} | ${sourceCell(entry.source, manifest[release])} | ${entry.data.status} | ${entry.implementation.status} | ${required(release).length} | ${pinCount(release)} | ${scenarios}, ${notOk === "none" ? "all ok" : notOk} | ${entry.admitted ? "yes" : "no"} | ${state} |`;
  });
  const families = [...new Set(INTERACTIONS.map((interaction) => interaction.family))];
  const familyRows = grouped(releases, (release) => families.map((family) => required(release).filter((item) => item.family === family).length).join(" | "));
  const lines = [
    "# Release status", "",
    "Generated by `node scripts/release-status-doc.cjs`: do not edit. `test/release-status-doc.test.cjs` fails when a release, a status or a pin differs from this page.", "",
    generatedLine, "",
    `There are ${releases.length} registered releases. ${admitted} are admitted. No release is certified to match vanilla. A release in this table runs. That is availability, not parity.`, "",
    "The list is `SUPPORTED_VERSIONS` of `native/src/versioned/mod.rs`, in its order. It is the same list as `src/versions.ts`, `generated/release-manifest.json` and `src/release-status.json`.", "",
    "## How to read a row", "",
    "| column | says | read from |", "|---|---|---|",
    "| source | whether the vanilla source can be cited: `exact`, `alias` (a proved alias), `blocked` (not reproducible), with its identity | `src/release-status.json`, `generated/release-manifest.json` |",
    "| data | whether the generated tables are the ones joined | `src/release-status.json` |",
    "| implementation | every interaction of the contract has its entry points and no declared gap. This is no statement of parity | `src/release-status.json` |",
    "| cells | contract cells the release must verify: interactions it has in the game | `scripts/parity-contract.cjs` |",
    "| pins | cells with a pin on record, current or not | `generated/conformance-evidence.json` |",
    "| last pass | scenarios in the index of the 2026-11-06 pass, and those not `ok`. A status there says a recording finished, not that it matches, and those recordings are not current | `docs/recorder-coverage.md` |",
    "| admitted | all four dimensions pass, which no flag sets | `scripts/release-admission.cjs` |", "",
    "## States", "",
    "| state | meaning | releases |", "|---|---|---:|",
    ...Object.entries(states).map(([state, count]) => `| ${state} | ${stateMeaning(state)} | ${count} |`), "",
    "A recorder scenario the engine has no model for is `unsupported`, per recording, never per release (the recorder's `replay/verdict.ts`). `docs/limitations.md` lists what is unsupported by family. Known failures are the recorder package's (its `known-failures.json`, @minecraft-physics-native/recorder): it has no entries, and `scripts/release-matrix.cjs` is red on any.", "",
    "## Releases", "",
    "| release | source | data | implementation | cells | pins | last pass | admitted | state |", "|---|---|---|---|---:|---:|---|---|---|",
    ...rows, "",
    "## Pins", "",
    `${pinned} pins are on record, from commit ${commits.map((commit) => `\`${commit}\``).join(", ")} on engine ${engines.map((engine) => `\`${engine}\``).join(", ")}. A pin counts only for the engine digest it names, and any change to \`native/src\` outside its tests makes it stale. Read whether one is current with \`node scripts/release-admission.cjs <release>\`, never from this page.`, "",
    "## Cells by family", "", `Each number is the cells of that family a release must verify, of the ${INTERACTIONS.length} in the contract.`, "",
    `| releases | ${families.join(" | ")} |`, `|---|${families.map(() => "---:").join("|")}|`,
    ...familyRows.map(([range, value]) => `| ${range} | ${value} |`), "",
    "## Source gaps", "",
    "- 1.8.8 to 1.13.2 come from each release's own client jar, remapped by the recorder's mapping and decompiled by vineflower 1.12.0, each held to its tree hash (`docs/source-records.md`). None is an alias or blocked.",
    "- 1.16.4 keeps `rejectedRef`: `origin/client1.16.4` embeds 20w45a and is never read. Its record comes from the client jar.",
    "- 26.1.2 comes from its client jar with vineflower 1.12.0. Its record has no remapped jar hash, as 26.x is unobfuscated.",
    "- 1.19.1, 1.21.2, 1.21.7, 1.21.10, 26.1.1 and 26.1.2 are exact releases of minecraft-data. Their tables, but the version and 1.19.1's language, are those of the release of the same protocol, proved identical in `generated/data-aliases.json` (`docs/registry-release-contract.md`).", "",
    "## What is open", "",
    "| what | ticket |", "|---|---|",
    "| Pins on the current engine, from the pinned corpus | B288 |",
    "| Every release through its completion gate | B288 |",
    "| Differential traces with no unexplained mismatch | B282 |",
    "| Exact registry data for the four aliased releases | B302 |",
    "| 1.13.2 `me_mob_phantom_circles`, intermittently not deterministic | B314 |",
    "| Linux, macOS and platform packaging | B264, B275 |", "",
  ];
  return `${lines.join("\n").trimEnd()}\n`;
}

function stateMeaning(state) {
  if (state.startsWith("blocked")) return "The source cannot be cited, so the release cannot be admitted whatever its traces show.";
  if (state === "unpinned: no conformance pin") return "The source and data pass. No conformance pin covers the release on the current engine; what its current replays show is generated/subsystem-matrix.json (the recorder's docs/subsystem-matrix.md renders it).";
  if (state.startsWith("unpinned")) return "As above, and the last recording pass left scenarios of the release unrecorded.";
  if (state === "admitted") return "All four dimensions pass.";
  return "A dimension other than conformance fails.";
}

function generatedLine() {
  const commit = spawnSync("git", ["rev-parse", "--short", "HEAD"], { cwd: root, encoding: "utf8" }).stdout.trim();
  const pins = Object.values(json("generated/conformance-evidence.json").releases).flatMap((cells) => Object.values(cells));
  const engine = engineDigest();
  const current = pins.filter((pin) => pin.engine === engine).length;
  return `Generated ${new Date().toISOString().slice(0, 10)} on commit \`${commit}\`, engine \`${engine.slice(0, 8)}\`. ${current} of ${pins.length} pins named that engine then.`;
}

/** The page without the line that says when it was written, which moves with every run. */
const withoutGeneratedLine = (text) => text.replace(/\r\n/g, "\n").replace(/^Generated \d{4}-.*$/m, "Generated");

if (require.main === module) {
  if (process.argv.includes("--check")) {
    if (withoutGeneratedLine(fs.readFileSync(page, "utf8")) !== withoutGeneratedLine(render(collect(), "Generated"))) {
      console.error("docs/release-status.md differs: node scripts/release-status-doc.cjs");
      process.exit(1);
    }
  } else {
    fs.writeFileSync(page, render(collect(), generatedLine()));
  }
}

module.exports = { collect, passRows, render, stateOf, withoutGeneratedLine };
