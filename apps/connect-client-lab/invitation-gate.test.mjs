import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createInvitationGate, clearInvitationEvent, InvitationGateError } from './invitation-gate.mjs';

// Pure contract fixtures only. These are NOT valid Ed25519 invitations. Real
// signature verification belongs to the pinned Rust guest, exercised by the
// existing real CLI tests in tunnel-session.test.mjs.
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const clone = value => structuredClone(value);

function fixture(t, profile = 'game') {
  const clock = { wall: 1_800_000_000_000, monotonic: 1000 };
  t.mock.method(Date, 'now', () => clock.wall);
  t.mock.method(performance, 'now', () => clock.monotonic);
  const host = 'a'.repeat(64); const guest = 'b'.repeat(64);
  const game = { serviceId: 'game', protocol: 'tcp', endpoint: '127.0.0.1:43001', revision: 1,
    requiredCapability: 'reliableStream', maxDatagramBytes: 1400, udpReturnPolicy: { type: 'replyBudget' } };
  const services = [game];
  if (profile !== 'game') services.push({ serviceId: 'aux', protocol: 'udp',
    endpoint: profile === 'svc' ? '127.0.0.1:43002' : '127.0.0.1:43003', revision: 1,
    requiredCapability: 'datagram', maxDatagramBytes: 1400,
    udpReturnPolicy: { type: 'authorizedDownstream', bytesPerSecond: 16384, datagramsPerSecond: 20 } });
  const plan = { schemaVersion: 1, bind: '127.0.0.1:31000', durationSeconds: 120, services };
  const planHash = hash(`${JSON.stringify(plan)}\n`);
  const gate = createInvitationGate({ plan, planHash });
  const hostEvent = { event: 'invite_host_identity', peer: host,
    plan: { schemaKind: 'localLabServicePlan', schemaVersion: 1, experimental: true,
      hashAlgorithm: 'sha256', hash: planHash, before: { session: 'notStarted' },
      after: { bind: plan.bind, durationSeconds: 120, services: clone(services) }, filesChanged: [],
      relay: false, modConfigurationChanges: false, authorization: 'requiresExplicitHashAndPinnedGuest',
      rollback: 'revokeAndStopLabProcess', minecraft: 'notVerified', simpleVoiceChat: 'notVerified', plasmoVoice: 'notVerified' },
    next: 'review services; give host public ID to guest through an authorized channel; enter guest public ID within 60 seconds' };
  const guestEvent = { event: 'guest_identity', peer: guest,
    next: 'authorize this public ID at the pinned host; paste its invitation within 60 seconds' };
  const grants = services.map(({ endpoint, ...grant }) => grant);
  const body = { schemaVersion: 1, hostKey: host, guestKey: guest, sessionId: host,
    invitationId: 'c'.repeat(64), issuedAtUnixMs: clock.wall, expiresAtUnixMs: clock.wall + 120_000,
    generation: 1, directAddress: plan.bind, services: grants };
  const encoded = JSON.stringify({ body, signature: 'd'.repeat(128) });
  const invitationEvent = { event: 'host_invitation', invitation: encoded, digest: hash(encoded), singleAdmission: true };
  const reviewEvent = { event: 'invitation_review', digest: hash(encoded),
    invitation: { schemaVersion: 1, experimental: true, invitationSha256: hash(encoded),
      hostKey: host, guestKey: guest, sessionId: host, invitationId: body.invitationId,
      issuedAtUnixMs: body.issuedAtUnixMs, expiresAtUnixMs: body.expiresAtUnixMs, generation: 1,
      directAddress: plan.bind, services: clone(grants), singleUse: true, relay: false, accountIdentityVerified: false },
    localTcp: '127.0.0.1:0', localUdp: profile === 'game' ? null : '127.0.0.1:35554',
    next: 'review pinned host, guest, services and expiry; enter approve followed by the exact digest' };
  const writes = { peer: 0, invitation: 0, approval: 0 };
  const peerWriter = value => { assert.ok(value === guest, 'unexpected_peer_write'); writes.peer++; };
  const invitationWriter = value => { assert.ok(value === encoded, 'unexpected_invitation_write'); writes.invitation++; };
  const approvalWriter = value => { assert.ok(value === `approve ${hash(encoded)}`, 'unexpected_approval_write'); writes.approval++; };
  function authorize() {
    assert.ok(gate.hostIdentity(hostEvent) === host, 'unexpected_host_pin');
    gate.guestIdentity(guestEvent); gate.authorizePeer(peerWriter);
  }
  function delivered() { authorize(); gate.invitation(invitationEvent, invitationWriter); }
  t.after(() => gate.dispose());
  return { gate, clock, plan, hostEvent, guestEvent, invitationEvent, reviewEvent,
    writes, peerWriter, invitationWriter, approvalWriter, authorize, delivered };
}

function refused(fn) {
  assert.throws(fn, error => {
    assert.ok(error instanceof InvitationGateError);
    assert.match(error.code, /^game_tunnel_(approval_mismatch|identity_invalid|invitation_expired)$/);
    assert.equal(error.message, error.code);
    assert.ok(!JSON.stringify(error).includes('127.0.0.1'));
    assert.ok(!JSON.stringify(error).includes('PRIVATE_SENTINEL'));
    assert.deepEqual(Object.keys(error).sort(), ['code', 'name']);
    return true;
  });
}

function rewrite(event, change) {
  const envelope = JSON.parse(event.invitation);
  change(envelope);
  event.invitation = JSON.stringify(envelope); event.digest = hash(event.invitation);
}

for (const profile of ['game', 'svc', 'plasmo']) {
  test(`${profile}: exact local plan, envelope and Rust review permit one approval only`, t => {
    const f = fixture(t, profile);
    assert.ok(Object.isFrozen(f.gate)); assert.equal(JSON.stringify(f.gate), '{}');
    f.delivered();
    assert.deepEqual(f.writes, { peer: 1, invitation: 1, approval: 0 });
    f.gate.review(f.reviewEvent);
    assert.equal(f.writes.approval, 0); // Pure test: observes approval writes only.
    f.gate.approve(f.approvalWriter);
    assert.equal(f.writes.approval, 1);
    refused(() => f.gate.approve(f.approvalWriter));
    assert.equal(f.writes.approval, 1);
  });
}

test('local plan snapshot cannot be broadened after gate construction', t => {
  const f = fixture(t, 'svc');
  f.plan.bind = '0.0.0.0:32000'; f.plan.services[1].endpoint = '203.0.113.1:43003';
  f.plan.services[1].udpReturnPolicy.bytesPerSecond = 999999;
  f.delivered(); f.gate.review(f.reviewEvent); f.gate.approve(f.approvalWriter);
  assert.equal(f.writes.approval, 1);
});

const hostChanges = {
  extra: e => { e.extra = 'PRIVATE_SENTINEL'; },
  invalidPin: e => { e.peer = 'A'.repeat(64); },
  planHash: e => { e.plan.hash = 'e'.repeat(64); },
  bind: e => { e.plan.after.bind = '0.0.0.0:31000'; },
  lifetime: e => { e.plan.after.durationSeconds = 121; },
  target: e => { e.plan.after.services[0].endpoint = '127.0.0.1:43002'; },
  wrongMod: e => { e.plan.after.services[1].endpoint = '127.0.0.1:43003'; },
  service: e => { e.plan.after.services[1].serviceId = 'foreign'; },
  policy: e => { e.plan.after.services[1].udpReturnPolicy.bytesPerSecond++; },
  nestedExtra: e => { e.plan.after.services[1].udpReturnPolicy.extra = true; },
  changes: e => { e.plan.filesChanged.push('PRIVATE_SENTINEL'); },
  relay: e => { e.plan.relay = true; },
  verifiedClaim: e => { e.plan.minecraft = 'verified'; },
  missing: e => { delete e.plan.before; },
};
for (const [name, mutate] of Object.entries(hostChanges)) {
  test(`host plan mismatch ${name} writes neither peer nor approval`, t => {
    const f = fixture(t, 'svc'); mutate(f.hostEvent);
    refused(() => f.gate.hostIdentity(f.hostEvent));
    refused(() => f.gate.authorizePeer(f.peerWriter));
    assert.deepEqual(f.writes, { peer: 0, invitation: 0, approval: 0 });
  });
}

for (const [name, mutate] of Object.entries({
  sameAsHost: e => { e.peer = 'a'.repeat(64); },
  invalid: e => { e.peer = 'b'.repeat(63); },
  extra: e => { e.extra = 'PRIVATE_SENTINEL'; },
  missing: e => { delete e.next; },
})) {
  test(`guest identity mismatch ${name} cannot authorize a peer`, t => {
    const f = fixture(t); f.gate.hostIdentity(f.hostEvent); mutate(f.guestEvent);
    refused(() => f.gate.guestIdentity(f.guestEvent));
    refused(() => f.gate.authorizePeer(f.peerWriter));
    assert.deepEqual(f.writes, { peer: 0, invitation: 0, approval: 0 });
  });
}

const bodyChanges = {
  hostPin: e => { e.body.hostKey = 'e'.repeat(64); },
  guestPin: e => { e.body.guestKey = 'e'.repeat(64); },
  session: e => { e.body.sessionId = 'e'.repeat(64); },
  externalAddress: e => { e.body.directAddress = '203.0.113.1:31000'; },
  otherLoopback: e => { e.body.directAddress = '127.0.0.1:31001'; },
  generation: e => { e.body.generation = 2; },
  schema: e => { e.body.schemaVersion = 2; },
  invalidId: e => { e.body.invitationId = 'PRIVATE_SENTINEL'; },
  extra: e => { e.extra = true; },
  bodyExtra: e => { e.body.extra = true; },
  privateTarget: e => { e.body.services[0].endpoint = '127.0.0.1:43001'; },
  service: e => { e.body.services[0].serviceId = 'foreign'; },
  revision: e => { e.body.services[0].revision = 2; },
  capability: e => { e.body.services[0].requiredCapability = 'datagram'; },
  protocol: e => { e.body.services[0].protocol = 'udp'; },
  mtu: e => { e.body.services[0].maxDatagramBytes++; },
  policy: e => { e.body.services[1].udpReturnPolicy.datagramsPerSecond++; },
  policyExtra: e => { e.body.services[1].udpReturnPolicy.extra = true; },
  missingAux: e => { e.body.services.pop(); },
  order: e => { e.body.services.reverse(); },
  expired: e => { e.body.expiresAtUnixMs = e.body.issuedAtUnixMs; },
  lifetime: e => { e.body.expiresAtUnixMs++; },
  future: e => { e.body.issuedAtUnixMs++; },
  stale: e => { e.body.issuedAtUnixMs--; },
  unsafeInteger: e => { e.body.expiresAtUnixMs = Number.MAX_SAFE_INTEGER + 1; },
  float: e => { e.body.issuedAtUnixMs += 0.5; },
  signatureLength: e => { e.signature = 'd'.repeat(127); },
  signatureUppercase: e => { e.signature = 'D'.repeat(128); },
};
for (const [name, mutate] of Object.entries(bodyChanges)) {
  test(`invitation ${name} refuses even when its outer SHA256 matches`, t => {
    const f = fixture(t, 'svc'); f.authorize(); rewrite(f.invitationEvent, mutate);
    refused(() => f.gate.invitation(f.invitationEvent, f.invitationWriter));
    refused(() => f.gate.review(f.reviewEvent));
    refused(() => f.gate.approve(f.approvalWriter));
    assert.deepEqual(f.writes, { peer: 1, invitation: 0, approval: 0 });
  });
}

for (const [name, mutate] of Object.entries({
  digest: e => { e.digest = 'f'.repeat(64); },
  admission: e => { e.singleAdmission = false; },
  extra: e => { e.extra = 'PRIVATE_SENTINEL'; },
  missing: e => { delete e.singleAdmission; },
  malformed: e => { e.invitation = '{'; e.digest = hash(e.invitation); },
  bytes: e => { e.invitation = 'é'.repeat(2049); e.digest = hash(e.invitation); },
  whitespace: e => { e.invitation += '\n'; e.digest = hash(e.invitation); },
  duplicate: e => { e.invitation = e.invitation.replace('"schemaVersion":1,', '"schemaVersion":1,"schemaVersion":1,'); e.digest = hash(e.invitation); },
  reordered: e => { const parsed = JSON.parse(e.invitation); e.invitation = JSON.stringify({ signature: parsed.signature, body: parsed.body }); e.digest = hash(e.invitation); },
  escape: e => { e.invitation = e.invitation.replace('"hostKey"', '"host\\u004bey"'); e.digest = hash(e.invitation); },
})) {
  test(`invalid envelope framing ${name} never reaches guest stdin`, t => {
    const f = fixture(t); f.authorize(); mutate(f.invitationEvent);
    refused(() => f.gate.invitation(f.invitationEvent, f.invitationWriter));
    assert.deepEqual(f.writes, { peer: 1, invitation: 0, approval: 0 });
  });
}

const reviewChanges = {
  digest: e => { e.digest = 'e'.repeat(64); },
  invitationHash: e => { e.invitation.invitationSha256 = 'e'.repeat(64); },
  host: e => { e.invitation.hostKey = 'e'.repeat(64); },
  guest: e => { e.invitation.guestKey = 'e'.repeat(64); },
  session: e => { e.invitation.sessionId = 'e'.repeat(64); },
  id: e => { e.invitation.invitationId = 'e'.repeat(64); },
  expiry: e => { e.invitation.expiresAtUnixMs++; },
  generation: e => { e.invitation.generation++; },
  address: e => { e.invitation.directAddress = '127.0.0.1:31001'; },
  service: e => { e.invitation.services[0].serviceId = 'foreign'; },
  budget: e => { e.invitation.services[1].udpReturnPolicy.bytesPerSecond++; },
  singleUse: e => { e.invitation.singleUse = false; },
  relay: e => { e.invitation.relay = true; },
  account: e => { e.invitation.accountIdentityVerified = true; },
  experimental: e => { e.invitation.experimental = false; },
  tcp: e => { e.localTcp = '0.0.0.0:0'; },
  udp: e => { e.localUdp = '127.0.0.1:35555'; },
  extra: e => { e.extra = 'PRIVATE_SENTINEL'; },
  missing: e => { delete e.invitation.singleUse; },
};
for (const [name, mutate] of Object.entries(reviewChanges)) {
  test(`review mismatch ${name} never writes approval`, t => {
    const f = fixture(t, 'svc'); f.delivered(); mutate(f.reviewEvent);
    refused(() => f.gate.review(f.reviewEvent));
    refused(() => f.gate.approve(f.approvalWriter));
    assert.deepEqual(f.writes, { peer: 1, invitation: 1, approval: 0 });
  });
}

for (const boundary of ['beforeReview', 'beforeApproval', 'backwardClock', 'monotonicDeadline']) {
  test(`expiry ${boundary} refuses without an approval write`, t => {
    const f = fixture(t); f.delivered();
    if (boundary !== 'beforeReview') f.gate.review(f.reviewEvent);
    if (boundary === 'backwardClock') f.clock.wall--;
    else if (boundary === 'monotonicDeadline') f.clock.monotonic += 120_000;
    else f.clock.wall += 120_000;
    if (boundary === 'beforeReview') refused(() => f.gate.review(f.reviewEvent));
    refused(() => f.gate.approve(f.approvalWriter));
    assert.equal(f.writes.approval, 0);
  });
}

test('phase skips, repeated review, disposal and failed review never permit approval', t => {
  for (const phase of ['fresh', 'host', 'guest', 'peer', 'invitation', 'repeatReview', 'disposed']) {
    const f = fixture(t);
    if (phase !== 'fresh') f.gate.hostIdentity(f.hostEvent);
    if (!['fresh', 'host'].includes(phase)) f.gate.guestIdentity(f.guestEvent);
    if (!['fresh', 'host', 'guest'].includes(phase)) f.gate.authorizePeer(f.peerWriter);
    if (['invitation', 'repeatReview', 'disposed'].includes(phase)) f.gate.invitation(f.invitationEvent, f.invitationWriter);
    if (phase === 'repeatReview') { f.gate.review(f.reviewEvent); refused(() => f.gate.review(f.reviewEvent)); }
    if (phase === 'disposed') { f.gate.review(f.reviewEvent); f.gate.dispose(); }
    refused(() => f.gate.approve(f.approvalWriter));
    assert.equal(f.writes.approval, 0);
  }
});

test('delivered event references can be cleared without weakening the stored expected review', t => {
  const f = fixture(t, 'svc'); f.delivered();
  clearInvitationEvent(f.hostEvent); clearInvitationEvent(f.guestEvent); clearInvitationEvent(f.invitationEvent);
  assert.deepEqual(f.hostEvent, {}); assert.deepEqual(f.guestEvent, {}); assert.deepEqual(f.invitationEvent, {});
  f.gate.review(f.reviewEvent); clearInvitationEvent(f.reviewEvent); f.gate.approve(f.approvalWriter);
  assert.equal(f.writes.approval, 1); assert.equal(JSON.stringify(f.gate), '{}');
});
