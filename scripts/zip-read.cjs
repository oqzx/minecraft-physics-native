// Reads a .jar without unpacking it: the central directory, and one entry at a time. Enough for
// what the source inputs check inside a client jar (version.json, class bytes).
const fs = require("node:fs");
const zlib = require("node:zlib");

const END_OF_DIRECTORY = 0x06054b50;
const DIRECTORY_ENTRY = 0x02014b50;

/** The entries of a zip: `{ names, read(name) }`, read returning the entry's bytes. */
function openZip(file) {
  const bytes = fs.readFileSync(file);
  let end = bytes.length - 22;
  while (end >= 0 && bytes.readUInt32LE(end) !== END_OF_DIRECTORY) end -= 1;
  if (end < 0) throw new Error(`${file} is not a zip`);
  const count = bytes.readUInt16LE(end + 10);
  let offset = bytes.readUInt32LE(end + 16);
  const entries = new Map();
  for (let index = 0; index < count; index += 1) {
    if (bytes.readUInt32LE(offset) !== DIRECTORY_ENTRY) throw new Error(`${file} has a broken directory`);
    const method = bytes.readUInt16LE(offset + 10);
    const compressedSize = bytes.readUInt32LE(offset + 20);
    const nameLength = bytes.readUInt16LE(offset + 28);
    const extraLength = bytes.readUInt16LE(offset + 30);
    const commentLength = bytes.readUInt16LE(offset + 32);
    const localHeader = bytes.readUInt32LE(offset + 42);
    entries.set(bytes.toString("utf8", offset + 46, offset + 46 + nameLength), { method, compressedSize, localHeader });
    offset += 46 + nameLength + extraLength + commentLength;
  }
  return {
    names: [...entries.keys()],
    read(name) {
      const entry = entries.get(name);
      if (!entry) return null;
      const start = entry.localHeader + 30 + bytes.readUInt16LE(entry.localHeader + 26) + bytes.readUInt16LE(entry.localHeader + 28);
      const data = bytes.subarray(start, start + entry.compressedSize);
      return entry.method === 0 ? Buffer.from(data) : zlib.inflateRawSync(data);
    },
  };
}

module.exports = { openZip };
