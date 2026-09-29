import { test } from 'node:test';
import assert from 'node:assert/strict';
import { join, resolve } from 'node:path';
import { mkdtemp, rmdir, symlink, unlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolveExplicitLocalPath } from './local-path.mjs';
import { readArtifactFile } from './artifact.mjs';
import { readConfig } from './inspect.mjs';

const refused = [
  '\\\\share.invalid\\private\\voice.jar', '//share.invalid/private/voice.jar',
  '\\\\?\\UNC\\share.invalid\\private\\voice.jar', '\\\\?\\C:\\private\\voice.jar',
  '\\\\.\\C:\\private\\voice.jar', '\\??\\C:\\private\\voice.jar',
  '\\private\\voice.jar', 'C:voice.jar', 'C:', 'file:///C:/private/voice.jar',
  'C:/private/voice.jar:stream', 'voice.jar:stream', 'voice.jar::$DATA',
  'NUL', 'sub/COM1.jar', 'sub/LPT¹.jar', 'C:/private/CONIN$', 'sub/trailing. /voice.jar',
  '', '\u0000', ...(process.platform === 'win32' ? ['/private/voice.jar'] : []),
];

test('network, device, drive-relative, alternate-stream and ambiguous paths fail lexically', () => {
  for (const path of refused) assert.throws(() => resolveExplicitLocalPath(path), { message: 'unsafe_local_path_syntax' });
});

test('conventional explicit relative and local absolute paths remain accepted', () => {
  for (const path of ['apps/connect-voice-tools/fixtures/svc.properties', './a normal folder/voice.jar', '../voice.jar', resolve('voice.jar')]) {
    assert.equal(resolveExplicitLocalPath(path), resolve(path));
  }
  if (process.platform === 'win32') {
    for (const path of ['C:/local/voice.jar', 'C:\\local\\voice.jar']) assert.equal(resolveExplicitLocalPath(path), resolve(path));
  }
});

test('both explicit readers reject unsafe syntax before stat or network filesystem errors', async () => {
  for (const path of refused) {
    await assert.rejects(readArtifactFile(path), { message: 'unsafe_local_path_syntax' });
    await assert.rejects(readConfig(path), { message: 'unsafe_local_path_syntax' });
  }
});

test('both readers reject a static parent junction before following it to a missing target', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'aurora-connect-parent-test-'));
  const link = join(directory, 'parent-link');
  try {
    // No remote connection: a nonexistent local target distinguishes rejection
    // at the observed parent from following it and receiving ENOENT for the file.
    await symlink(join(directory, 'missing-target'), link, process.platform === 'win32' ? 'junction' : 'dir');
    await assert.rejects(readConfig(join(link, 'config.properties')), { message: 'linked_local_parent' });
    await assert.rejects(readArtifactFile(join(link, 'voice.jar')), { message: 'linked_local_parent' });
  } finally {
    await unlink(link).catch(() => {});
    await rmdir(directory);
  }
});
