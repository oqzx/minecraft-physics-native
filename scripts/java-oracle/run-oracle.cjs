// Compiles an oracle that names no class of the jars and runs it on a release, the jar and its libraries on the class path:
//   node scripts/java-oracle/run-oracle.cjs CLASS RELEASE ARGS...
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { classpath } = require('./classpath.cjs');

function compile(name) {
  const classes = fs.mkdtempSync(path.join(os.tmpdir(), `oracle-${name}-`));
  const done = spawnSync('javac', ['-nowarn', '-d', classes, path.join(__dirname, `${name}.java`)], { encoding: 'utf8' });
  if (done.status !== 0) throw new Error(done.stderr);
  return classes;
}

function run(classes, name, release, args) {
  const { jars } = classpath(release);
  const result = spawnSync('java', ['-Xmx1500m', '-XX:-OmitStackTraceInFastThrow', '-XX:+UnlockDiagnosticVMOptions', '-XX:-BytecodeVerificationRemote',
    '--add-opens', 'java.base/java.lang=ALL-UNNAMED', '-cp', [classes, ...jars].join(path.delimiter), name, ...args],
  { encoding: 'utf8', maxBuffer: 1 << 28 });
  if (result.status !== 0) throw new Error(`${name} ${release}: ${(result.stderr ?? '').slice(0, 3000)}`);
  return result.stdout;
}

if (require.main === module) {
  const [name, release, ...args] = process.argv.slice(2);
  const classes = compile(name);
  process.stdout.write(run(classes, name, release, args));
  fs.rmSync(classes, { recursive: true, force: true });
}

module.exports = { compile, run };
