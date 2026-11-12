// Every native function outside the tests, with its callers and the contract families that reach it
// (B018, docs/shared-code-audit.md). It complements generated/shared-gates.json, which lists the gated
// functions: most shared code has no gate at all, and serves every release from the same lines.
//
// A function's interval is computed: an ungated function serves every registered release from one
// body, a gated one has the boundaries of its gates. Its status says what backs that:
//   entry            an exported N-API function, not itself a claim about vanilla
//   source-compared  gated, and the source audit finds a change at each boundary
//   unverified       anything else, which is the incomplete marker: it claims nothing
// Callers are matched by name in the source, so a name shared by two functions counts for both, and a
// call through a generic or a function pointer can be missed: the map over-reports rather than proves.
// This is computed when asked and not checked in: it changes with nearly every native commit.
//   node scripts/shared-functions.cjs              a summary by file
//   node scripts/shared-functions.cjs <function>   one function: callers, families, interval
const fs = require("node:fs");
const path = require("node:path");
const { INTERACTIONS } = require("./parity-contract.cjs");
const { codeLines, sharedGates } = require("./shared-gates.cjs");

const root = path.resolve(__dirname, "..");
const sourceRoot = path.join(root, "native", "src");
const snake = (name) => name.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`);

/** The functions of one source: each with its body, its test module left out. */
function functionsOf(text, file) {
  const out = [];
  let current = null;
  const usable = codeLines(text);
  usable.forEach((line, index) => {
    const declared = /^\s*(?:pub(?:\([a-z]+\))? )?(?:const )?(?:unsafe )?fn (\w+)/.exec(line);
    if (declared) {
      current = { file, name: declared[1], line: index + 1, napi: /^\s*#\[napi/.test(usable[index - 1] ?? ""), body: [] };
      out.push(current);
    } else if (current) current.body.push(line);
  });
  return out;
}

/** The call graph of the native sources and each function's reach from the contract's entry points. */
function sharedFunctions() {
  const files = fs.readdirSync(sourceRoot).filter((name) => name.endsWith(".rs") && !/tests\.rs$/.test(name)).sort();
  const versioned = path.join("versioned", "mod.rs");
  const all = [...files, versioned].flatMap((file) => functionsOf(fs.readFileSync(path.join(sourceRoot, file), "utf8"), file.replaceAll("\\", "/")));
  const byName = new Map();
  for (const entry of all) byName.set(entry.name, [...(byName.get(entry.name) ?? []), entry]);
  const calls = new Map(all.map((entry) => {
    const called = new Set();
    for (const [, name] of entry.body.join("\n").matchAll(/\b([a-z_][a-z0-9_]*)(?:::<[^>]*>)?\(/g)) {
      if (byName.has(name) && name !== entry.name) called.add(name);
    }
    return [entry, called];
  }));
  const callers = new Map(all.map((entry) => [entry.name, new Set()]));
  for (const [entry, called] of calls) for (const name of called) callers.get(name).add(entry.name);

  const families = new Map(all.map((entry) => [entry.name, new Set()]));
  for (const interaction of INTERACTIONS) {
    const pending = interaction.api.map(snake).filter((name) => byName.has(name));
    const seen = new Set(pending);
    while (pending.length > 0) {
      const name = pending.pop();
      families.get(name).add(interaction.family);
      for (const entry of byName.get(name)) {
        for (const next of calls.get(entry)) {
          if (!seen.has(next)) {
            seen.add(next);
            pending.push(next);
          }
        }
      }
    }
  }
  const gates = new Map(sharedGates().map((gate) => [`${gate.file}:${gate.function}`, gate]));
  return all.map((entry) => {
    const gate = gates.get(`${entry.file}:${entry.name}`);
    return {
      file: entry.file,
      function: entry.name,
      line: entry.line,
      callers: [...callers.get(entry.name)].sort(),
      families: [...families.get(entry.name)].sort(),
      interval: gate ? gate.boundaries : "every release",
      status: entry.napi ? "entry" : gate?.status === "source-compared" ? "source-compared" : "unverified",
    };
  });
}

if (require.main === module) {
  const entries = sharedFunctions();
  const wanted = process.argv[2];
  if (wanted) console.log(JSON.stringify(entries.filter((entry) => entry.function === wanted), null, 2));
  else {
    const totals = {};
    for (const entry of entries) {
      totals[entry.file] ??= { functions: 0, reached: 0, ungated: 0, "source-compared": 0 };
      totals[entry.file].functions += 1;
      if (entry.families.length > 0) totals[entry.file].reached += 1;
      if (entry.interval === "every release") totals[entry.file].ungated += 1;
      if (entry.status === "source-compared") totals[entry.file]["source-compared"] += 1;
    }
    console.table(totals);
  }
}

module.exports = { functionsOf, sharedFunctions };
