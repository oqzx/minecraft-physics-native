'use strict';

// The `js doctest` blocks of the documents run as written, each page's blocks in one scope (B270, B271).
// Run after `pnpm build` and the native build: node --test test/

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { EventEmitter } = require('node:events');
const { it } = require('node:test');
const adapter = require('..');

const binding = adapter.LoadNativePhysics();

for (const page of ['synchronization.md', 'state-contract.md', 'field-catalogue.md']) {
    it(`runs the examples of docs/${page}`, () => {
        const text = fs.readFileSync(path.join(__dirname, '..', 'docs', page), 'utf8').replace(/\r\n/g, '\n');
        const blocks = [...text.matchAll(/^```js doctest\n([\s\S]*?)^```$/gm)].map(match => match[1]);
        assert.ok(blocks.length >= 2, `${page} has its examples`);
        const scoped = new Function('assert', 'binding', 'EventEmitter', 'NativeWorldSync', 'require', blocks.join('\n{\n').replace(/$/, `\n${'}'.repeat(blocks.length - 1)}`));
        scoped(assert, binding, EventEmitter, adapter.NativeWorldSync, require);
    });
}
