// Runs a wasm32-wasip1 program of this crate under Node's WASI, with the working directory open to it:
// node scripts/java-oracle/wasm/run.mjs path/to/oracle-wasm.wasm [arguments]
import { readFile } from 'node:fs/promises';
import { WASI } from 'node:wasi';

const [path, ...args] = process.argv.slice(2);
const wasi = new WASI({ version: 'preview1', args: ['oracle-wasm', ...args], preopens: { '.': '.' } });
const module = await WebAssembly.compile(await readFile(path));
// A wasm32-wasip1-threads build imports its memory, shared: it is made here, and the program itself spawns no thread.
const imports = wasi.getImportObject();
if (WebAssembly.Module.imports(module).some(i => i.module === 'env' && i.name === 'memory')) {
    imports.env = { memory: new WebAssembly.Memory({ initial: 4096, maximum: 16384, shared: true }) };
}
const instance = await WebAssembly.instantiate(module, imports);
process.exitCode = wasi.start(instance);
