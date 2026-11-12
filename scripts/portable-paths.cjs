// Paths a generated manifest records. They are written with forward slashes relative to the
// manifest's directory, whichever host wrote them, and read back against that directory, so a
// manifest means the same files on Windows, Linux and macOS. A root on another Windows drive has
// no relative form and stays absolute. Neither form is a Git object name (`<ref>:<path>`), which
// stays as it is.
const nodePath = require("node:path");

const WINDOWS_ABSOLUTE = /^[A-Za-z]:[\\/]|^\\\\/;

/** `target` as a manifest in `manifestDirectory` records it. */
function serializePath(manifestDirectory, target, pathApi = nodePath) {
  return pathApi.relative(manifestDirectory, target).replaceAll("\\", "/");
}

/**
 * The directory or file a manifest in `manifestDirectory` means by `recorded`: forward slashes,
 * or the backslashes of a manifest from before they were normalized. A Windows root read on
 * another host names nothing there and is an error, never a file with a colon in its name.
 */
function resolveRecordedPath(manifestDirectory, recorded, pathApi = nodePath) {
  if (typeof recorded !== "string" || recorded.length === 0) {
    throw new Error(`A manifest in ${manifestDirectory} records no path: ${JSON.stringify(recorded)}`);
  }
  if (WINDOWS_ABSOLUTE.test(recorded) && pathApi.sep !== "\\") {
    throw new Error(`${recorded} in a manifest in ${manifestDirectory} is a Windows path; set the source root for this host`);
  }
  return pathApi.resolve(manifestDirectory, recorded.replaceAll("\\", "/"));
}

module.exports = { serializePath, resolveRecordedPath };
