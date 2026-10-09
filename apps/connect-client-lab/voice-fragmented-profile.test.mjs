import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fragmentedReady, fragmentedSnapshot, createFragmentedEventSequence } from './voice-fragmented-profile.mjs';

const ready = () => ({ event: 'guest_ready', tcp: '127.0.0.1:30000', udp: '127.0.0.1:35554',
  profile: 'fragmentedV2', udpEnvelopeVersion: 2, approvedOriginalDatagramBytes: 4096,
  transportOriginalCapacity: 4096, associationCeilingNegotiated: true, capacityScope: 'initialAssociation', maxFragmentFrameBytes: 1264,
  experimental: true, relay: false,
  next: 'enter stop then press Enter to close; invitation whole-session expiry is unchanged' });
const snapshot = () => ({ event: 'fragmented_transport_snapshot', experimental: true, profile: 'fragmentedV2',
  datagramFramesTxSinceReady: 17, datagramFramesRxSinceReady: 19,
  evidence: 'transport_counters_only_not_minecraft_or_voice' });

test('V2 readiness distinguishes full original capacity from frames and scopes negotiation to the initial association', () => {
  const event = ready(); const context = fragmentedReady(event);
  assert.ok(Object.isFrozen(context)); assert.equal(context.bridgePort, 30000);
  assert.equal(context.approvedOriginalDatagramBytes, 4096); assert.equal(context.transportOriginalCapacity, 4096);
  assert.equal(context.maxFragmentFrameBytes, 1264); assert.equal(context.associationCeilingNegotiated, true);
  assert.equal(context.capacityScope, 'initialAssociation');
  assert.equal(context.maxDatagramPayload, undefined); assert.equal(context.next, undefined);
  event.tcp = '0.0.0.0:1'; assert.equal(context.bridgePort, 30000);
});

for (const [name, mutate] of Object.entries({
  externalTcp: e => { e.tcp = '0.0.0.0:30000'; },
  tcpZero: e => { e.tcp = '127.0.0.1:0'; },
  tcpOverflow: e => { e.tcp = '127.0.0.1:65536'; },
  changedUdp: e => { e.udp = '127.0.0.1:35555'; },
  legacy: e => { e.profile = 'legacy'; },
  version: e => { e.udpEnvelopeVersion = 1; },
  approved: e => { e.approvedOriginalDatagramBytes = 1400; },
  capacity: e => { e.transportOriginalCapacity = 4095; },
  unavailable: e => { e.transportOriginalCapacity = null; },
  frame: e => { e.maxFragmentFrameBytes = 4096; },
  negotiation: e => { e.associationCeilingNegotiated = false; },
  scope: e => { e.capacityScope = 'allFutureAssociations'; },
  missingScope: e => { delete e.capacityScope; },
  relay: e => { e.relay = true; },
  extra: e => { e.voiceAudio = 'tested'; },
  missing: e => { delete e.experimental; },
})) {
  test(`V2 readiness refuses ${name}`, () => {
    const event = ready(); mutate(event);
    assert.throws(() => fragmentedReady(event), { message: 'game_tunnel_mode_invalid' });
  });
}

function guestSequence() {
  const sequence = createFragmentedEventSequence('guest');
  for (const event of ['guest_identity', 'invitation_review', 'guest_ready']) sequence.accept({ event }, false);
  return sequence;
}
test('V2 snapshot alone does not complete cleanup; exact stopped event after closing is still required', () => {
  const sequence = guestSequence(); assert.equal(sequence.finished, false);
  sequence.accept(snapshot(), true); assert.equal(sequence.finished, false);
  assert.equal(sequence.transport.datagramFramesTxSinceReady, 17);
  sequence.accept({ event: 'guest_stopped' }, true); assert.equal(sequence.finished, true);
});
for (const boundary of ['beforeClosing', 'missing', 'duplicate', 'earlyStopped', 'extraStopped', 'earlySnapshot']) {
  test(`V2 terminal sequence ${boundary} fails closed`, () => {
    const sequence = boundary === 'earlySnapshot' ? createFragmentedEventSequence('guest') : guestSequence();
    if (['duplicate', 'extraStopped'].includes(boundary)) sequence.accept(snapshot(), true);
    const event = ['missing', 'earlyStopped'].includes(boundary) ? { event: 'guest_stopped' }
      : boundary === 'extraStopped' ? { event: 'guest_stopped', audio: true } : snapshot();
    assert.throws(() => sequence.accept(event, !['beforeClosing', 'earlyStopped'].includes(boundary)));
    assert.equal(sequence.finished, false); assert.equal(sequence.transport, undefined);
    assert.throws(() => sequence.accept({ event: 'guest_stopped' }, true));
  });
}
test('V2 host cannot emit a guest snapshot, or stop before caller cleanup', () => {
  const sequence = createFragmentedEventSequence('host');
  for (const event of ['invite_host_identity', 'host_invitation', 'host_ready']) sequence.accept({ event }, false);
  assert.throws(() => sequence.accept(snapshot(), true)); assert.equal(sequence.finished, false);
  const host = createFragmentedEventSequence('host');
  for (const event of ['invite_host_identity', 'host_invitation', 'host_ready']) host.accept({ event }, false);
  assert.throws(() => host.accept({ event: 'host_stopped' }, false)); assert.equal(host.finished, false);
});

test('V2 transport snapshot is a frozen copy with only redacted QUIC frame counters', () => {
  const event = snapshot(); const value = fragmentedSnapshot(event);
  assert.ok(Object.isFrozen(value)); assert.deepEqual(value, event);
  event.datagramFramesTxSinceReady = 0; assert.equal(value.datagramFramesTxSinceReady, 17);
  assert.doesNotMatch(JSON.stringify(value), /peer|session|127\.0\.0\.1/);
});
for (const [name, mutate] of Object.entries({
  txNegative: e => { e.datagramFramesTxSinceReady = -1; },
  rxFloat: e => { e.datagramFramesRxSinceReady = 1.5; },
  txUnsafe: e => { e.datagramFramesTxSinceReady = Number.MAX_SAFE_INTEGER + 1; },
  profile: e => { e.profile = 'legacy'; },
  evidence: e => { e.evidence = 'voice_delivered'; },
  extra: e => { e.peer = 'PRIVATE_SENTINEL'; },
  missing: e => { delete e.experimental; },
})) {
  test(`V2 transport snapshot refuses ${name}`, () => {
    const event = snapshot(); mutate(event);
    assert.throws(() => fragmentedSnapshot(event), { message: 'game_tunnel_mode_invalid' });
  });
}
