// node scripts/decompile-legacy-trees.cjs DIR: each legacy release's own client, remapped by the recorder's mapping, decompiled for audit-random-draws --legacy DIR.
const { classpath } = require('./java-oracle/classpath.cjs');
const { decompilerJar } = require('./source-pins.cjs');
const { spawnSync } = require('child_process');
const fs = require('fs');
const root = process.argv[2];
if (!root) throw new Error('usage: node scripts/decompile-legacy-trees.cjs DIR');
for (const v of ['1.8.8', '1.9.4', '1.10.2', '1.11.2', '1.12.2', '1.13.2']) {
  const out = `${root}/${v}`;
  if (fs.existsSync(`${out}/done`)) continue;
  fs.mkdirSync(out, { recursive: true });
  const { jars, client } = classpath(v);
  const args = ['-Xmx2g', '-jar', decompilerJar(), '--thread-count=2', '--log-level=error', '--folder', jars[0], out];
  const started = Date.now();
  const r = spawnSync('java', args, { encoding: 'utf8', maxBuffer: 1 << 28 });
  console.log(v, client, 'status', r.status, Math.round((Date.now() - started) / 1000) + 's', (r.stderr || '').slice(0, 300));
  if (r.status === 0) fs.writeFileSync(`${out}/done`, `${client}\n`);
}
