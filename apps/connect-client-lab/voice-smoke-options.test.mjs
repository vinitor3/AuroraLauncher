import { test } from 'node:test';
import assert from 'node:assert/strict';
import { voiceSmokeOptions } from './voice-smoke-options.mjs';
import { fragmentedVoiceContract } from './voice-fragmented-profile.mjs';

for (const profile of ['svc', 'plasmo']) {
  test(`${profile} keeps both legacy smoke modes separate from fragmented transport`, () => {
    assert.deepEqual(voiceSmokeOptions(profile, ['--offline-lab']), { keepalive: false, fragmented: false });
    assert.deepEqual(voiceSmokeOptions(profile, ['--offline-lab', '--keepalive']), { keepalive: true, fragmented: false });
  });

  test(`${profile} requires the explicit ordered opt-in and the reviewed artifact contract`, () => {
    const transport = fragmentedVoiceContract();
    for (const keepalive of [false, true]) {
      const args = ['--offline-lab', '--fragmented-udp', ...(keepalive ? ['--keepalive'] : [])];
      const options = voiceSmokeOptions(profile, args);
      assert.deepEqual(options, { keepalive, fragmented: true, transport });
      assert.ok(Object.isFrozen(options)); assert.ok(Object.isFrozen(options.transport));
      assert.throws(() => { options.transport.approvedOriginalDatagramBytes = 8192; }, TypeError);
      assert.equal(options.transport.approvedOriginalDatagramBytes, 4096);
      assert.equal(options.transport.udpEnvelopeVersion, 2);
      assert.equal(options.transport.invitationSchemaVersion, 1);
    }
  });

  test(`${profile} refuses extra flags, reordered opt-in, caller pins and missing offline consent`, () => {
    for (const args of [undefined, [], ['--fragmented-udp'], ['--keepalive'], ['--offline-lab', '--fragmented-udp=true'],
      ['--offline-lab', '--keepalive', '--fragmented-udp'], ['--offline-lab', '--fragmented-udp', '--fragmented-udp'],
      ['--offline-lab', '--fragmented-udp', '--keepalive', '--keepalive'], ['--offline-lab', '--fragmented-udp', '4096'],
      ['--offline-lab', '--fragmented-udp', '--exe=PRIVATE'], ['--offline-lab', '--fragmented-udp', 'a'.repeat(64)],
      ['--offline-lab', '--fragmented-udp', '--relay'], ['--offline-lab', '--fragmented-udp', '--audio']]) {
      assert.throws(() => voiceSmokeOptions(profile, args), { message: `client_${profile}_usage` });
    }
  });
}

test('voice smoke refuses a profile other than the two independently approved mods', () => {
  for (const profile of ['game', 'voice', 'svc-fragmented', null]) {
    assert.throws(() => voiceSmokeOptions(profile, ['--offline-lab']), { message: 'client_voice_profile' });
  }
});
