// test/fixtures/contexts, test/fixtures/legacy-neighbours and test/fixtures/legacy-block-entities: OracleContext.java on every
// release from 1.14, LegacyNeighbourDump.java on the six before, the second with block entities in its level. They need the
// release's jars, so they are made here and checked in:
//   node scripts/java-oracle/generate-fixtures.cjs [contexts|neighbours|entities] [RELEASE...]
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const zlib = require('node:zlib');
const { compile, run } = require('./run-oracle.cjs');
const { registeredReleases, compareReleases } = require('../registered-releases.cjs');

const fixtures = path.resolve(__dirname, '..', '..', 'test', 'fixtures');
const [kind = 'contexts', ...only] = process.argv.slice(2);
const oracle = { contexts: 'OracleContext', neighbours: 'LegacyNeighbourDump', entities: 'LegacyNeighbourDump' }[kind];
const folder = path.join(fixtures, { contexts: 'contexts', neighbours: 'legacy-neighbours', entities: 'legacy-block-entities' }[kind]);
const wanted = (version) => (kind === 'contexts') === (compareReleases(version, '1.14') >= 0);
const releases = only.length > 0 ? only : registeredReleases().map(({ version }) => version).filter(wanted);

fs.mkdirSync(folder, { recursive: true });
const classes = compile(oracle);
const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'oracle-fixture-'));
for (const version of releases) {
  const output = path.join(scratch, `${version}.tsv`);
  run(classes, oracle, version, kind === 'entities' ? [output, 'entities'] : [output]);
  // (the line ends of the platform are no part of the answer)
  const text = fs.readFileSync(output, 'utf8').replaceAll('\r\n', '\n');
  fs.writeFileSync(path.join(folder, `${version}.tsv.gz`), zlib.gzipSync(text, { level: 9 }));
  console.log(version, text.length);
}
fs.rmSync(classes, { recursive: true, force: true });
fs.rmSync(scratch, { recursive: true, force: true });
