'use strict';

// B271: what the field contracts cite is checked by machine, row by row, not by sample. Each vanilla anchor resolves to a file
// and line that declares the member it names, in the first and the newest release that has it; each binding rule names code
// this repository has; each `why` resolves to files and lines that exist; and a boundary that bounds a row is a real one.
// The source lines are read where this machine has the sources (scripts/field-evidence.cjs says where); the rest of the
// checks need nothing but the repository.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { MECHANIC_ANCHORS, contracts, isAnchor } = require('../scripts/field-contracts.cjs');
const { declarations, parseAnchor, sourceBase } = require('../scripts/field-evidence.cjs');
const { READ_EDGES, readEdge } = require('../scripts/field-read-edges.cjs');

const root = path.resolve(__dirname, '..');
const evidence = JSON.parse(fs.readFileSync(path.join(root, 'docs', 'field-evidence.json'), 'utf8'));
const all = contracts();
const rows = Object.entries(all).flatMap(([name, { fields }]) => Object.entries(fields).map(([field, row]) => ({ name: `${name}.${field}`, row })));

/** A place recorded as `release path:line`, read back: { checked, problem }. */
const read = new Map();
function declares(place, anchor) {
    const [, release, file, line] = /^(\S+) (.+):(\d+)$/.exec(place) ?? [];
    if (!release) return { checked: false, problem: `${place} is no place` };
    const base = sourceBase(release);
    if (!base) return { checked: false, problem: null };
    const full = path.join(base, file);
    if (!fs.existsSync(full)) return { checked: true, problem: `${place}: no such file` };
    const groups = parseAnchor(anchor);
    const className = path.basename(file, '.java');
    const group = groups.find(({ classes }) => classes.includes(className));
    if (!group) return { checked: true, problem: `${place} is no class of ${anchor}` };
    if (group.members.length === 0) return { checked: true, problem: Number(line) === 1 ? null : `${place}: a class is cited at its first line` };
    if (!read.has(full)) read.set(full, declarations(fs.readFileSync(full, 'utf8')));
    const declared = read.get(full);
    const held = group.members.some(member => declared.get(member) === Number(line));
    return { checked: true, problem: held ? null : `${place} declares no ${group.members.join(' or ')}` };
}

describe('the field contracts\' citations', () => {
    it('resolve each vanilla anchor to the line that declares its member, first and newest', t => {
        const problems = [];
        let checked = 0;
        for (const [anchor, record] of Object.entries(evidence.records)) {
            for (const place of [record.oldest, record.at]) {
                const result = declares(place, anchor);
                if (result.checked) checked += 1;
                if (result.problem) problems.push(`${anchor}: ${result.problem}`);
            }
        }
        if (checked === 0) return t.skip('no source tree on this machine');
        assert.deepEqual(problems, []);
        assert.equal(checked, Object.keys(evidence.records).length * 2, 'every recorded place was read back');
    });

    it('records every anchor the contracts cite, from the legacy trees to 26.3', () => {
        const cited = [...new Set(rows.map(({ row }) => row[2]).filter(isAnchor))].sort();
        assert.deepEqual(Object.keys(evidence.records).sort(), cited, 'node scripts/field-evidence.cjs');
        for (const release of ['1.8.8', '1.12.2', '1.13.2', '1.14.4', '26.3']) assert.ok(evidence.sources.includes(release), release);
    });

    it('gives each anchor where its member came and went by identity, within its named range', () => {
        const order = evidence.sources;
        const at = release => order.indexOf(release);
        for (const [anchor, record] of Object.entries(evidence.records)) {
            assert.ok(['oldest', 'added', 'unresolved'].includes(record.since.by), `${anchor} since ${record.since.by}`);
            assert.ok(['newest', 'removed', 'unresolved'].includes(record.until.by), `${anchor} until ${record.until.by}`);
            assert.ok(at(record.since.release) <= at(record.first) && at(record.until.release) >= at(record.last), anchor);
            if (record.since.by === 'oldest') assert.equal(record.since.release, order[0], anchor);
            if (record.until.by === 'newest') assert.equal(record.until.release, order.at(-1), anchor);
        }
    });

    it('bounds a row only by a boundary vanilla really has: MECHANIC_ANCHORS is no rename and no guess', () => {
        const list = [...MECHANIC_ANCHORS];
        assert.deepEqual(list, [...list].sort(), 'kept sorted');
        const cited = new Set(rows.map(({ row }) => row[2]));
        for (const anchor of list) {
            assert.ok(cited.has(anchor), `${anchor} is cited by no row`);
            const record = evidence.records[anchor];
            assert.ok(record, anchor);
            // a member that reaches both ends by identity was only renamed, and an unresolved edge is no evidence
            assert.ok(record.since.by === 'added' || record.until.by === 'removed', `${anchor}: ${record.since.by}..${record.until.by}`);
        }
    });

    it('reads in the trees each edge no mapping follows, and leaves no lower bound unchecked', () => {
        // each reading is of the edge the record has it for, the release beside it in the sources' order, and turned it to
        // `added`; where this machine has the tree, the reading holds again (its files declare none of the names, the holder
        // is declared)
        const order = evidence.sources;
        for (const [anchor, entry] of Object.entries(READ_EDGES)) {
            const record = evidence.records[anchor];
            assert.ok(record, `${anchor} is cited by no row`);
            assert.equal(record.since.release, entry.release, anchor);
            assert.equal(order[order.indexOf(entry.release) - 1], entry.beside, anchor);
            assert.equal(record.since.by, 'added', anchor);
            assert.ok(record.since.note.startsWith(`read in ${entry.beside}: `), anchor);
            const base = sourceBase(entry.beside);
            if (!base) continue;
            const { problems, holder } = readEdge(entry, base, declarations);
            assert.deepEqual(problems, [], anchor);
            if (entry.holder) assert.ok(record.since.note.endsWith(`(${entry.beside} ${holder})`), `${anchor}: ${holder}`);
        }
        // a row's lower bound the sources could not check is marked with a dagger: there is none, so a new anchor whose
        // identity is lost below its first release fails here until its edge is read
        const catalogue = fs.readFileSync(path.join(root, 'docs', 'field-catalogue.md'), 'utf8');
        assert.deepEqual(catalogue.split('\n').filter(line => /^\|.*\d † \|/.test(line)), []);
    });

    it('names in each binding rule code or files this repository has', () => {
        const ignored = new Set(['node_modules', 'target', '.git', 'dist', 'out', 'artifacts', 'build', '.gradle', 'logs', 'run']);
        const walk = at => fs.readdirSync(at, { withFileTypes: true }).flatMap(entry => (entry.isDirectory()
            ? (ignored.has(entry.name) ? [] : walk(path.join(at, entry.name)))
            : [path.join(at, entry.name).split(path.sep).join('/')]));
        const files = [...walk(root), path.join(root, 'native', 'artifacts', 'native.d.ts').split(path.sep).join('/')];
        const code = files.filter(file => /\.(rs|ts|cjs|java)$/.test(file));
        const words = new Set(code.flatMap(file => fs.readFileSync(file, 'utf8').match(/[A-Za-z_][A-Za-z0-9_]*/g) ?? []));
        const codeLike = word => (/_/.test(word) && /[a-z]/.test(word)) || /[a-z][A-Z]/.test(word) || /^[A-Z][a-z]+[A-Z]/.test(word);
        const problems = [];
        let named = 0;
        for (const { name, row } of rows) {
            if (isAnchor(row[2])) continue;
            const rule = row[2].replace(/^binding:\s*/, '');
            const cited = [...rule.matchAll(/([\w./-]+\.(?:rs|ts|cjs|md|json|java))(?::(\d+))?/g)];
            const missingFiles = cited.filter(([, file, line]) => {
                const hit = files.find(candidate => candidate.endsWith(`/${file}`));
                return !hit || (line && fs.readFileSync(hit, 'utf8').split('\n').length < Number(line));
            }).map(([text]) => text);
            const identifiers = (rule.replace(/[\w./-]+\.(?:rs|ts|cjs|md|json|java)(?::\d+)?/g, ' ').match(/[A-Za-z_][A-Za-z0-9_]*/g) ?? []).filter(codeLike);
            const missing = identifiers.filter(word => !words.has(word));
            if (missing.length || missingFiles.length || identifiers.length + cited.length === 0) problems.push(`${name}: ${missing.join(', ')} ${missingFiles.join(', ')}`.trim());
            named += 1;
        }
        assert.deepEqual(problems, []);
        assert.ok(named > 250, `${named} binding rows`);
    });

    it('resolves the files and lines each `why` cites, where this machine has the trees', () => {
        const listing = new Map();
        const java = at => fs.readdirSync(at, { withFileTypes: true }).flatMap(entry => (entry.isDirectory() ? java(path.join(at, entry.name)) : entry.name.endsWith('.java') ? [path.join(at, entry.name).split(path.sep).join('/')] : []));
        const find = (release, file) => {
            for (const tree of [sourceBase(release)].filter(at => at && fs.existsSync(at))) {
                if (!listing.has(tree)) listing.set(tree, java(tree));
                const hit = listing.get(tree).find(candidate => candidate.endsWith(`/${file}`));
                if (hit) return hit;
            }
            return null;
        };
        const hasTree = release => [sourceBase(release)].some(at => at && fs.existsSync(at));
        const repo = file => ['', 'native/src/', 'src/mineflayer/', 'scripts/'].map(prefix => path.join(root, prefix + file)).find(at => fs.existsSync(at)) ?? null;
        const problems = [];
        for (const { name, row } of rows) {
            const why = row[3]?.why;
            if (!why) continue;
            let release = null;
            let last = null;
            for (const match of why.matchAll(/(?:(\d+\.\d+(?:\.\d+)?)\s+(?:[\w/.-]*?\s)?)?([\w/$.-]+\.(?:java|rs|ts|cjs|md))?(?::([\d,\- ]+\d))?/g)) {
                if (match[1]) release = match[1];
                const [, , file, lines] = match;
                if (!file && !lines) continue;
                const hit = file ? (/\.java$/.test(file) ? (release ? find(release, file) : null) : repo(file)) : last;
                if (file && !hit) {
                    if (!/\.java$/.test(file) || (release && hasTree(release))) problems.push(`${name}: no ${release ?? ''} ${file}`);
                    continue;
                }
                if (!hit) continue;
                last = hit;
                const length = fs.readFileSync(hit, 'utf8').split('\n').length;
                for (const number of (lines ?? '').split(/[,\- ]+/).filter(Boolean)) if (Number(number) > length) problems.push(`${name}: ${file ?? path.basename(hit)}:${number} is past its ${length} lines`);
            }
        }
        assert.deepEqual(problems, []);
    });
});
