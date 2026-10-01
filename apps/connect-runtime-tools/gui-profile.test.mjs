import { test } from 'node:test';
import assert from 'node:assert/strict';
import { validateGuiNames, guiOfflineUuid, guiWhitelist } from './gui-profile.mjs';
import { PLAY_NAMES, offlineUuid, runtimeFiles, runtimeArtifacts } from './play-profile.mjs';

test('GUI whitelist accepts only one or two exact ASCII names and rejects casefold collisions', () => {
  for (const names of [['Ab_'], ['Ab_0123456789abc'], ['AuroraGuiA', 'AuroraGuiB']]) {
    assert.deepEqual(validateGuiNames(names), names);
  }
  for (const names of [undefined, null, 'AuroraGuiA', [], ['AuroraGuiA', 'AuroraGuiB', 'AuroraGuiC'],
    ['AuroraGuiA', 'AuroraGuiA'], ['AuroraGuiA', 'auroraguia'], ['ab'], ['a'.repeat(17)],
    [' abc'], ['abc '], ['abc\n'], ['abc\r'], ['abc\0'], ['abc-d'], ['abc.d'], ['abc/d'],
    ['abc\\d'], ['abc:d'], ['Ábc'], ['Ａbc'], ['😀abc'], ['--flag'], [123], [undefined], new Array(1)]) {
    assert.throws(() => validateGuiNames(names), { message: 'runtime_gui_names' });
  }
});

test('GUI offline UUID matches independently computed MD5 vectors and preserves exact case', () => {
  // Vectors independently computed with .NET MD5, UTF-8 and UUID v3/variant bits.
  assert.equal(guiOfflineUuid('AuroraGuiA'), '56e1d47e-7303-3574-a984-65bb14fbea93');
  assert.equal(guiOfflineUuid('AuroraGuiB'), 'f52ad692-9009-3194-b4d6-d5941a28587e');
  assert.equal(guiOfflineUuid('auroraguia'), '87cbda6d-281f-3789-b068-21d95ac25814');
  for (const name of PLAY_NAMES) assert.equal(guiOfflineUuid(name), offlineUuid(name));
  assert.throws(() => offlineUuid('AuroraGuiA'), /runtime_play_identity/);
  assert.throws(() => guiOfflineUuid('abc\n'), /runtime_gui_names/);
});

test('GUI files snapshot reviewed names while preserving play properties, nine artifacts and false EULA', () => {
  const custom = ['AuroraGuiA'];
  custom[Symbol.iterator] = function* () { assert.fail('caller iterator must not be consulted'); };
  custom.toJSON = () => assert.fail('caller serializer must not be consulted');
  assert.deepEqual(validateGuiNames(custom), ['AuroraGuiA']);
  const names = ['AuroraGuiA', 'AuroraGuiB'];
  const validated = validateGuiNames(names); const files = runtimeFiles('gui', names);
  names[0] = 'IndependentEdit';
  assert.ok(Object.isFrozen(validated)); assert.equal(validated[0], 'AuroraGuiA');
  assert.deepEqual(JSON.parse(files['whitelist.json']), [
    { uuid: '56e1d47e-7303-3574-a984-65bb14fbea93', name: 'AuroraGuiA' },
    { uuid: 'f52ad692-9009-3194-b4d6-d5941a28587e', name: 'AuroraGuiB' },
  ]);
  assert.equal(files['whitelist.json'], guiWhitelist(validated));
  assert.equal(files['server.properties'], runtimeFiles('play')['server.properties']);
  assert.equal(files['eula.txt'], runtimeFiles('status')['eula.txt']);
  assert.deepEqual(runtimeArtifacts('gui'), runtimeArtifacts('status'));
  assert.equal(runtimeArtifacts('gui').length, 9);
  for (const profile of ['status', 'play', 'svc', 'plasmo']) {
    assert.throws(() => runtimeFiles(profile, validated), /runtime_gui_names/);
  }
});
