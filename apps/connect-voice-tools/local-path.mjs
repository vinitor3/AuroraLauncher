import { dirname, resolve } from 'node:path';
import { lstat } from 'node:fs/promises';

// Lexical checks run before any filesystem API: a stat on UNC can initiate SMB.
// This is a local read policy, not a defense against a concurrent parent reparse
// change or a mapped drive whose remote backing is unknown to this process.
export function resolveExplicitLocalPath(value) {
  const fail = () => { throw new Error('unsafe_local_path_syntax'); };
  const check = path => {
    if (typeof path !== 'string' || !path || path.length > 32767 || /[\u0000-\u001f\u007f]/.test(path)) fail();
    if (/^[\\/]{2}/.test(path) || path.startsWith('\\')) fail();
    const drive = /^[a-zA-Z]:[\\/]/.test(path);
    if (/^[a-zA-Z]:/.test(path) && !drive) fail();
    if (path.slice(drive ? 2 : 0).includes(':')) fail();
    if (process.platform === 'win32' && path.startsWith('/')) fail();
    // Win32 aliases may address devices even with an extension or trailing dots.
    for (const part of path.slice(drive ? 3 : 0).split(/[\\/]/)) {
      if (part === '.' || part === '..' || !part) continue;
      if (/[ .]$/.test(part) || /^(?:con|prn|aux|nul|conin\$|conout\$|com[1-9¹²³]|lpt[1-9¹²³])(?:\.|$)/i.test(part)) fail();
    }
  };
  check(value);
  const absolute = resolve(value);
  check(absolute); // A relative input must not silently inherit a UNC cwd.
  return absolute;
}

// Walk parents top-down. lstat of the complete filename would already follow a
// parent junction (including one pointing at a remote share) before realpath.
// This rejects observed static links; it is not atomic against parent replacement.
export async function rejectLinkedParents(absolute) {
  const parents = [];
  let parent = dirname(absolute);
  for (;;) {
    parents.push(parent);
    const next = dirname(parent);
    if (next === parent) break;
    parent = next;
  }
  for (const path of parents.reverse()) {
    const metadata = await lstat(path);
    if (metadata.isSymbolicLink()) throw new Error('linked_local_parent');
    if (!metadata.isDirectory()) throw new Error('invalid_local_parent');
  }
}
