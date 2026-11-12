// Reads the engine's and the adapter's code for generated/movement-sites.json (B046): each numeric literal with the
// function and the field it stands in, the releases the `if release.at_least(...)` around it leave, and the citations of
// vanilla's sources beside it. Nothing here reads a vanilla source: scripts/generate-movement-sites.cjs does.

const FLOAT = /(?<![\w.])\d[\d_]*\.\d[\d_]*(?:[eE][-+]?\d+)?(?:_?f(?:32|64))?(?![\w.])|(?<![\w.])\d[\d_]*\.(?![\w.])|(?<![\w.])\d[\d_]*[eE][-+]?\d+(?:_?f(?:32|64))?(?![\w.])|(?<![\w.])\d[\d_]*_?f(?:32|64)(?![\w.])/g;
const TS_FLOAT = /(?<![\w.$'"])\d[\d_]*\.\d+(?:[eE][-+]?\d+)?(?![\w.])|(?<![\w.$])\d+[eE][-+]?\d+(?![\w.])/g;
// (a whole number: `1..3` is two, a tuple's `.0` none, and a float is the patterns above)
const WHOLE = /(?<![\w.]|[eE][-+])\d[\d_]*(?:_?[iu](?:8|16|32|64|128|size))?(?![\w]|\.\d|[eE][-+]?\d)/g;
const TS_WHOLE = /(?<![\w.$'"]|[eE][-+])\d[\d_]*(?![\w$]|\.\d|[eE][-+]?\d)/g;
/** What holds a whole number that is no constant of the game: a gate's release, an attribute, an array's length, an index. */
const NOT_A_CONSTANT = /(?:\bat_least|MinecraftVersion::new)\([^()]*\)|#!?\[[^\]\n]*\]|;\s*\d+\s*\]|\[\s*\d+\s*\]/g;
const masked = (code) => code.replace(NOT_A_CONSTANT, (text) => " ".repeat(text.length));

/** The code of a Rust or TypeScript file, comments and strings blanked with their lines kept, and the comments by line. */
function strip(text) {
  const code = [];
  const comments = new Map();
  const note = (line, comment) => comments.set(line, `${comments.get(line) ?? ""} ${comment}`);
  let line = 1;
  let at = 0;
  while (at < text.length) {
    const here = text[at];
    const next = text[at + 1];
    if (here === "/" && next === "/") {
      const end = text.indexOf("\n", at);
      const stop = end < 0 ? text.length : end;
      note(line, text.slice(at + 2, stop));
      at = stop;
    } else if (here === "/" && next === "*") {
      const end = text.indexOf("*/", at + 2);
      const stop = end < 0 ? text.length : end + 2;
      for (const part of text.slice(at + 2, stop).split("\n")) {
        note(line, part);
        code.push("\n");
        line += 1;
      }
      code.pop();
      line -= 1;
      at = stop;
    } else if (here === "\"" || here === "`") {
      let end = at + 1;
      while (end < text.length && text[end] !== here) end += text[end] === "\\" ? 2 : 1;
      for (const char of text.slice(at, end + 1)) {
        if (char === "\n") {
          code.push("\n");
          line += 1;
        }
      }
      code.push("\"\"");
      at = end + 1;
    } else if (here === "'" && /^'(?:[^\\']|\\.[^']*)'/.test(text.slice(at, at + 12))) {
      const match = /^'(?:[^\\']|\\.[^']*)'/.exec(text.slice(at, at + 12));
      code.push("' '");
      at += match[0].length;
    } else {
      if (here === "\n") line += 1;
      code.push(here);
      at += 1;
    }
  }
  return { code: code.join(""), comments };
}

const GATE = /^(!?)\s*(?:[\w.()]*\.)?(?:release|version)\.at_least\((\d+),\s*(\d+),\s*(\d+)\)$|^(!?)\s*(?:[\w.()]*\.)?at_least\((\d+),\s*(\d+),\s*(\d+)\)$/;

/** Splits on a separator outside of parentheses. */
function topLevel(text, separator) {
  const parts = [];
  let depth = 0;
  let start = 0;
  for (let at = 0; at < text.length; at += 1) {
    if ("([{".includes(text[at])) depth += 1;
    else if (")]}".includes(text[at])) depth -= 1;
    else if (depth === 0 && text.startsWith(separator, at)) {
      parts.push(text.slice(start, at));
      start = at + separator.length;
    }
  }
  parts.push(text.slice(start));
  return parts.map((part) => part.trim());
}

const unwrap = (text) => {
  let term = text.trim();
  while (term.startsWith("(") && term.endsWith(")") && topLevel(term.slice(1, -1), "\0").length === 1) {
    let depth = 0;
    let whole = true;
    for (let at = 0; at < term.length - 1; at += 1) {
      if (term[at] === "(") depth += 1;
      else if (term[at] === ")") depth -= 1;
      if (depth === 0) whole = false;
    }
    if (!whole) break;
    term = term.slice(1, -1).trim();
  }
  return term;
};

/** One term as a gate: `{ at, from }` holds from `at` when `from`, until it when not; null when it is no gate. */
function gateOf(term, names) {
  let text = unwrap(term);
  let negated = false;
  while (text.startsWith("!")) {
    negated = !negated;
    text = unwrap(text.slice(1));
  }
  const named = names.get(text);
  if (named) return { at: named.at, from: named.from !== negated };
  const match = GATE.exec(text);
  if (!match) return null;
  const numbers = (match[2] ? match.slice(2, 5) : match.slice(6, 9)).map(Number);
  return { at: numbers, from: !negated };
}

/** What a condition says of the release: [what holds in its branch, what holds in its else], each a list of gates. */
function conditionGates(condition, names) {
  const text = unwrap(condition);
  const any = topLevel(text, "||");
  const all = topLevel(text, "&&");
  if (any.length > 1 && all.length > 1) return [[], []];
  if (any.length > 1) {
    const gates = any.map((term) => gateOf(term, names)).filter(Boolean);
    return [[], gates.map(({ at, from }) => ({ at, from: !from }))];
  }
  const gates = all.map((term) => gateOf(term, names)).filter(Boolean);
  const inverted = all.length === 1 ? gates.map(({ at, from }) => ({ at, from: !from })) : [];
  return [gates, inverted];
}

/** The numeric literals of `code`, each with its function, field and gates. */
function walkSites(code, comments, literal, { typescript = false, whole = null } = {}) {
  const lines = code.split("\n");
  const offsets = [];
  let total = 0;
  for (const part of lines) {
    offsets.push(total);
    total += part.length + 1;
  }
  const lineAt = (offset) => {
    let low = 0;
    let high = offsets.length - 1;
    while (low < high) {
      const middle = Math.ceil((low + high) / 2);
      if (offsets[middle] <= offset) low = middle;
      else high = middle - 1;
    }
    return low + 1;
  };
  const names = new Map();
  const stack = [];
  const functions = [];
  const events = [];
  for (const match of code.matchAll(literal)) events.push({ at: match.index, text: match[0] });
  if (whole) for (const match of masked(code).matchAll(whole)) events.push({ at: match.index, text: match[0], whole: true });
  events.sort((a, b) => a.at - b.at);
  let event = 0;
  let boundary = 0;
  let afterIf = null;
  // (a `;` inside brackets ends no statement: `-> [f32; 2] {` still heads its function)
  let nest = 0;
  const sites = [];
  const context = () => {
    for (let index = stack.length - 1; index >= 0; index -= 1) if (stack[index].fn) return stack[index].fn;
    return null;
  };
  const gatesHere = () => stack.flatMap((frame) => frame.gates);
  const emit = (at, text, isWhole) => {
    const line = lineAt(at);
    const before = code.slice(Math.max(0, at - 240), at);
    const owner = /(?:^|\n)\s*(?:pub(?:\([a-z]+\))?\s+)?(?:const|static)\s+([A-Z][A-Z0-9_]*)\b[^;]*$/.exec(before);
    const field = /(?:^|[{,\n])\s*(?:pub\s+)?([a-z_]\w*):\s*[^,;{}\n]*$/.exec(before.split("\n").slice(-1)[0]);
    const fn = context();
    const after = code.slice(at + text.length, at + text.length + 8);
    const narrowed = /(?:java_float|\bf32|fround)\(-?$/.test(before.slice(-14)) || /^\s+as f32/.test(after);
    sites.push({ text, line, narrowed, ...(isWhole ? { whole: true } : {}), fn: fn?.name ?? (owner ? `const ${owner[1]}` : "(file)"), field: field?.[1] ?? null, gates: gatesHere(), fnIndex: fn?.index ?? -1 });
  };
  for (let at = 0; at < code.length; at += 1) {
    while (event < events.length && events[event].at <= at) {
      emit(events[event].at, events[event].text, events[event].whole);
      event += 1;
    }
    const char = code[at];
    if (char === "(" || char === "[") nest += 1;
    else if (char === ")" || char === "]") nest = Math.max(0, nest - 1);
    if (char === ";" && nest === 0) {
      const statement = code.slice(boundary, at);
      const binding = /\blet\s+(?:mut\s+)?(\w+)\s*(?::\s*bool\s*)?=\s*([\s\S]+)$/.exec(statement);
      if (binding) {
        const [gates] = conditionGates(binding[2], names);
        if (gates.length === 1) names.set(binding[1], gates[0]);
        else names.delete(binding[1]);
      }
      boundary = at + 1;
      afterIf = null;
    } else if (char === "{") {
      const head = code.slice(boundary, at);
      const frame = { gates: [], fn: null, chain: null, nest };
      nest = 0;
      const declared = typescript
        ? /(?:^|\n)\s*(?:(?:public|private|protected|static|async|override|readonly|get|set)\s+)*(?:function\s+)?(\w+)\s*(?:<[^>]*>)?\([^;{}]*\)\s*(?::\s*[^{;=]+)?$/.exec(head)
        : /\bfn\s+(\w+)/.exec(head);
      if (declared && !["if", "for", "while", "switch", "catch"].includes(declared[1])) {
        frame.fn = { name: declared[1], index: functions.length, line: lineAt(boundary + declared.index), end: 0 };
        functions.push(frame.fn);
        names.clear();
      }
      const elseHead = /\belse\s*(?:if\b([\s\S]*))?$/.exec(head);
      const ifHead = /\bif\b(?!\s+let\b)([\s\S]*)$/.exec(head);
      if (elseHead && afterIf) {
        frame.gates.push(...afterIf.other);
        if (elseHead[1] !== undefined) {
          const [gates, other] = conditionGates(elseHead[1], names);
          frame.gates.push(...gates);
          frame.chain = { other: [...afterIf.other, ...other] };
        }
      } else if (ifHead) {
        const [gates, other] = conditionGates(ifHead[1], names);
        frame.gates.push(...gates);
        frame.chain = { other };
      }
      stack.push(frame);
      boundary = at + 1;
      afterIf = null;
    } else if (char === "}") {
      const frame = stack.pop();
      nest = frame?.nest ?? 0;
      if (frame?.fn) frame.fn.end = lineAt(at);
      afterIf = frame?.chain ?? null;
      boundary = at + 1;
    } else if (afterIf && !/\s/.test(char)) {
      if (!afterIf.elseSeen && !code.startsWith("else", at)) afterIf = null;
      else afterIf.elseSeen = true;
    }
  }
  return { sites, functions };
}

const RELEASE = /^(?:client)?((?:1\.\d+|2\d\.\d+)(?:\.\d+)?)$/;

/** The citations of a comment: `{ release, file, from, to }`, a later `:a-b` or `Other.java` keeping the release before it. */
function citationsOf(comment) {
  const found = [];
  let release = null;
  let file = null;
  for (const match of comment.matchAll(/(?:client)?(?:1\.\d+|2\d\.\d+)(?:\.\d+)?\b|\b([A-Z]\w*)\.java\b|\b([A-Z][a-z]+[A-Z]\w*|[A-Z][a-z]{3,})\.[a-z]\w*|:(\d+)(?:\s*-\s*(\d+))?/g)) {
    const token = match[0];
    const version = RELEASE.exec(token);
    if (version) {
      release = version[1];
      continue;
    }
    if (match[1] || match[2]) {
      file = match[1] ?? match[2];
      const cited = { release, file, from: null, to: null };
      if (!found.some((item) => item.release === release && item.file === file && item.from === null)) found.push(cited);
      continue;
    }
    if (match[3] && file) {
      const from = Number(match[3]);
      const to = Number(match[4] ?? match[3]);
      found.push({ release, file, from, to: to < from ? from : to });
    }
  }
  return found;
}

/** The comments that speak for a function: its doc block, and those inside it. */
function functionComments(fn, comments) {
  const parts = [];
  for (let line = fn.line - 1; line > 0 && line > fn.line - 40; line -= 1) {
    if (!comments.has(line)) {
      if (line < fn.line - 3) break;
      continue;
    }
    parts.unshift(comments.get(line));
  }
  for (let line = fn.line; line <= fn.end; line += 1) if (comments.has(line)) parts.push(comments.get(line));
  return parts.join(" ");
}

/** The code with each `#[cfg(test)] mod` blanked to its last brace, lines kept: a test module in the middle of a file too. */
function withoutTestModules(code, comments) {
  let out = code;
  const opening = /#\[cfg\(test\)\]\s*mod\s+\w+\s*\{/g;
  for (let match = opening.exec(out); match; match = opening.exec(out)) {
    let depth = 0;
    let end = match.index + match[0].length - 1;
    for (; end < out.length; end += 1) {
      if (out[end] === "{") depth += 1;
      else if (out[end] === "}" && --depth === 0) break;
    }
    const first = out.slice(0, match.index).split("\n").length;
    const blanked = out.slice(match.index, end + 1).replace(/[^\n]/g, " ");
    for (let line = first; line < first + blanked.split("\n").length; line += 1) comments.delete(line);
    out = out.slice(0, match.index) + blanked + out.slice(end + 1);
  }
  return out;
}

function scanRust(text) {
  const stripped = strip(text);
  const comments = stripped.comments;
  const code = withoutTestModules(stripped.code, comments);
  const { sites, functions } = walkSites(code, comments, FLOAT, { whole: WHOLE });
  return { sites, functions, comments, code };
}

function scanTypeScript(text) {
  const { code, comments } = strip(text);
  const { sites, functions } = walkSites(code, comments, TS_FLOAT, { typescript: true, whole: TS_WHOLE });
  return { sites, functions, comments };
}

const valueOf = (text) => Number(text.replace(/_?(?:f(?:32|64)|[iu](?:8|16|32|64|128|size))$/, "").replaceAll("_", ""));
const suffixOf = (text) => /f32$/.test(text) ? "f32" : /f64$/.test(text) ? "f64" : null;

module.exports = { strip, conditionGates, walkSites, citationsOf, functionComments, scanRust, scanTypeScript, valueOf, suffixOf };
