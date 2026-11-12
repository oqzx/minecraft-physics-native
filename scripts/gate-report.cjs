// docs/version-gates.md and generated/gate-mutation.json, from a sweep of scripts/gate-mutants.cjs:
//   node scripts/gate-report.cjs <dir> [--commit <id>]
// A sweep flips each gate on the release either side of its boundary. A side is `caught` when a test or a replay stopped
// matching, `reached` when something ran the gate there and nothing noticed, `unreached` when nothing ran it. The json is a
// record of one sweep, of the commit it names: a gate added since has no entry and that is not an error. The commit is the
// pin: the sweep's gates must be the gates of native/src at that commit (scripts/gate-mutants.cjs sweepProblems), which
// test/shared-gates.test.cjs holds the committed record to, so a sweep of another tree, or a mislabelled one, is refused.
// Fixtures added since the sweep are flipped again by `gate-mutants.cjs recheck`, into unit-*.jsonl beside unit.jsonl (and
// js-*.jsonl beside js.jsonl), read the same way. A side no fixture can catch because the flip changes nothing reachable is
// `justified` when audit/gate-reviews.json holds a reviewed reason for it, keyed by the site's number in the sweep, with the
// vanilla lines read on both sides; a review must name the gate it was written for, so a renumbered sweep refuses it.
const fs = require("node:fs");
const path = require("node:path");
const { compareReleases } = require("./registered-releases.cjs");
const { gateCitations } = require("./gate-citations.cjs");
const { sweepProblems } = require("./gate-mutants.cjs");

const root = path.resolve(__dirname, "..");
const [directory] = process.argv.slice(2);
if (!process.argv.includes("--commit")) throw new Error("--commit <id>: the commit the sweep was prepared from");
const commit = process.argv[process.argv.indexOf("--commit") + 1];
const target = path.resolve(directory);
const sites = JSON.parse(fs.readFileSync(path.join(target, "sites.json"), "utf8"));
const unswept = sweepProblems(sites, commit);
if (unswept.length > 0) throw new Error(`the sweep in ${target} is not one of ${commit}:\n  ${unswept.slice(0, 20).join("\n  ")}`);
const lines = (file) => (fs.existsSync(file) ? fs.readFileSync(file, "utf8").split("\n").filter(Boolean).map((line) => JSON.parse(line)) : []);
const results = new Map();
const of = (site, release) => {
  const key = `${site}:${release}`;
  if (!results.has(key)) results.set(key, { ran: 0 });
  return results.get(key);
};
const named = (pattern) => fs.readdirSync(target).filter((file) => pattern.test(file)).sort();
for (const entry of named(/^unit(-.*)?\.jsonl$/).flatMap((file) => lines(path.join(target, file)))) {
  const result = of(entry.site, entry.release);
  result.ran += entry.tests;
  if (entry.killedBy) result.caughtBy ??= `rust: ${entry.killedBy}`;
}
for (const entry of named(/^js(-.*)?\.jsonl$/).flatMap((file) => lines(path.join(target, file)))) {
  const result = of(entry.site, entry.release);
  result.ran += entry.files;
  if (entry.killedBy) result.caughtBy ??= `test: ${entry.killedBy}`;
}
for (const name of fs.readdirSync(target).filter((file) => /^replay-.*\.jsonl$/.test(file))) {
  for (const entry of lines(path.join(target, name))) {
    const result = of(entry.site, entry.release);
    result.ran += entry.reaching;
    // a replay is the better witness: it is vanilla's
    if (entry.killedBy) result.caughtBy = `replay: ${entry.killedBy}`;
  }
}
const reviewsPath = path.join(root, "audit", "gate-reviews.json");
const reviews = fs.existsSync(reviewsPath) ? JSON.parse(fs.readFileSync(reviewsPath, "utf8")).reviews : {};
const REF = /^\d+(?:\.\d+)+ [\w$/.]+\.java:\d+(?:-\d+)?/;
const reviewProblems = Object.entries(reviews).flatMap(([number, review]) => {
  const site = sites[Number(number)];
  if (!site) return [`${number}: no such site in the sweep`];
  const problems = [];
  for (const key of ["file", "function", "boundary", "text"]) if (review[key] !== site[key]) problems.push(`${number}: ${key} is ${JSON.stringify(review[key])}, the sweep's site has ${JSON.stringify(site[key])}`);
  if (!Array.isArray(review.sides) || review.sides.length === 0 || review.sides.some((release) => release !== site.lastBefore && release !== site.firstFrom)) problems.push(`${number}: sides must be ${site.lastBefore} and/or ${site.firstFrom}`);
  if (typeof review.reason !== "string" || review.reason.length < 40) problems.push(`${number}: no reason`);
  if (!Array.isArray(review.refs) || review.refs.length < 2 || review.refs.some((ref) => !REF.test(ref))) problems.push(`${number}: refs must be two or more "<release> <File>.java:<line>"`);
  return problems;
});
if (reviewProblems.length > 0) throw new Error(`audit/gate-reviews.json:\n  ${reviewProblems.join("\n  ")}`);
const side = (site, release) => {
  if (!release) return "none";
  const result = of(site, release);
  if (result.caughtBy) return "caught";
  if (reviews[site]?.sides.includes(release)) return "justified";
  return result.ran > 0 ? "reached" : "unreached";
};
const entries = sites.map((site) => ({
  file: site.file,
  function: site.function,
  boundary: site.boundary,
  lastBefore: site.lastBefore,
  firstFrom: site.firstFrom,
  before: { state: side(site.site, site.lastBefore), by: site.lastBefore ? of(site.site, site.lastBefore).caughtBy : undefined },
  after: { state: side(site.site, site.firstFrom), by: of(site.site, site.firstFrom).caughtBy },
  review: reviews[site.site] ? `audit/gate-reviews.json#${site.site}` : undefined,
}));
fs.writeFileSync(path.join(root, "generated", "gate-mutation.json"), `${JSON.stringify({ schema: 1, generatedBy: "scripts/gate-report.cjs", commit, entries }, null, 1)}\n`);

// (the citations are the code's as it is now, not the sweep's: scripts/gate-citations.cjs)
const current = gateCitations();
const boundaries = [...new Set(sites.map((site) => site.boundary))].sort(compareReleases);
const count = (list, test) => list.filter(test).length;
// a side with no release (a gate below the first registered one) is never flipped, and stands as answered
const settled = (state) => state === "caught" || state === "justified" || state === "none";
const both = (entry) => entry.before.state === "caught" && entry.after.state === "caught";
const justified = (entry) => !both(entry) && settled(entry.before.state) && settled(entry.after.state);
const one = (entry) => !both(entry) && !justified(entry) && (entry.before.state === "caught" || entry.after.state === "caught");
const none = (entry) => !both(entry) && !justified(entry) && !one(entry) && (entry.before.state === "reached" || entry.after.state === "reached");
const away = (entry) => !both(entry) && !justified(entry) && !one(entry) && !none(entry);
const rows = boundaries.map((boundary) => {
  const held = entries.filter((entry) => entry.boundary === boundary);
  const first = held[0];
  return `| ${boundary} | ${held.length} | ${first.lastBefore} | ${first.firstFrom} | ${count(held, both)} | ${count(held, justified)} | ${count(held, one)} | ${count(held, none)} | ${count(held, away)} |`;
});
const citedCount = current.filter((gate) => gate.cited).length;
const document = `# Version gates

Every \`at_least\` gate of the engine outside tests, by the release it starts at, and what happens when it is wrong. Generated by
\`scripts/gate-report.cjs\` from a sweep of commit \`${commit}\`. "Last before" and "first from" are the registered releases around
the boundary. ${sites.length} gates in the sweep. Of the ${current.length} in the code now, ${citedCount} cite a line of their release or of
the registered release either side in their function's comments (\`scripts/gate-citations.cjs\`, held by \`test/gate-citations.test.cjs\`).

\`scripts/gate-mutants.cjs\` builds a copy of the engine in which each gate can be flipped on one release, which is the gate moved
one release that way. Each was flipped on both of its releases against the Rust tests and \`test/*.test.cjs\` that run it on that
release, and against up to 60 of the recordings of that release that run it, from every recording of the corpus that
the recorder's \`corpus/\` pins, on all 50 releases (the recorder's \`replay/gate-sweep.ts --cap 60\`); a gate that left a side unnoticed was swept again against up to 300, with
every recording's moving entities and projectiles replayed too (\`--wide\`). Fixtures added after the sweep for the gates it
left were flipped again on both sides against them (\`gate-mutants.cjs recheck\`). "Both" and "one" count the gates whose move was
noticed on both sides or on one, "justified" those whose every side was noticed or holds a reviewed reason, with vanilla's lines
on both sides, that the move changes nothing reachable on that release (\`audit/gate-reviews.json\`), "none" those something ran
on a side and nothing noticed, "not run" those nothing ran on either side. A side that is not noticed is not a wrong gate: some
differ only in a case no fixture reaches (B290).

| gate | sites | last before | first from | both | justified | one | none | not run |
|---|---:|---|---|---:|---:|---:|---:|---:|
${rows.join("\n")}

\`generated/gate-mutation.json\` holds each gate's two sides and what noticed them.

## Removed gates

Each decided nothing, so each could go. \`scripts/gate-equivalence.cjs\` ran one fixed battery on all 50 releases, on the final engine
and on a copy of it with all of these put back: 216 runs each of a player, a boat and four moving entities at the world border and
into walls, the knockback around both thresholds, and the capabilities. None of the 50 differ, and nothing was refused. The battery
is not blind to these: moving \`collide_vehicle\`'s 1.18 border gate one release either way changes the boat's section on that
release alone, and moving the knockback's 1.17 float root or its 1.21 early return changes the knockback's. It does not see the
player's own 1.18 border gate moved, so for \`collide_with_world\` the last row of this table rests on reading it.

| Gate | Where | Why it decided nothing | What holds the rule now |
|---|---|---|---|
| \`supports_fluid_physics\`, \`supports_honey_blocks\`, \`supports_fall_flying\` | \`version.rs\` | Nothing called them | The capabilities' fields, on the same boundaries |
| \`minor_horizontal_collision: at_least(1, 18, 0)\` | \`version.rs\` | The same test twice | \`supports_minor_horizontal_collision\` |
| \`at_least(1, 14, 0)\` round the border's join, \`at_least(1, 13, 0)\` on \`border_always_collides\` | \`collide_vehicle\` | \`run_vehicle_move\` takes \`legacy_collide\` below 1.14 | The \`modern\` gate of \`run_vehicle_move\` |
| \`at_least(1, 21, 0)\` choosing \`1.0E-5\` | \`living_knockback\` | 1.21 has returned already for a length that small (1.21 LivingEntity.java:1469) | The 1.21 early return |
| \`border_always_collides\` | \`CollisionRules\`, set in \`collide_with_world\` and \`collide_vehicle\` | Only \`collide\` read it, and neither calls \`collide\`: both pass the border in \`Colliders\` | \`Colliders::border\` |
`;
fs.writeFileSync(path.join(root, "docs", "version-gates.md"), document);
console.log(`${entries.length} gates: ${count(entries, both)} both, ${count(entries, justified)} justified, ${count(entries, one)} one, ${count(entries, none)} none, ${count(entries, away)} not run; ${citedCount} of ${current.length} cited`);
