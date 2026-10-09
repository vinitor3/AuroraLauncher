// Local caller checks for the pinned Rust CLI invitation contract. This module
// does NOT verify Ed25519 or authenticate an account: the guest CLI performs
// signature verification before emitting its review. No I/O occurs here.
import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { FRAGMENTED_PROFILE, FRAGMENT_FRAME_BYTES, ORIGINAL_DATAGRAM_BYTES, PREPARATION_SECONDS,
  FRAGMENTED_HOST_NEXT, FRAGMENTED_GUEST_NEXT, FRAGMENTED_REVIEW_NEXT } from './voice-fragmented-profile.mjs';

const KEY = /^[a-f0-9]{64}$/;
const SIGNATURE = /^[a-f0-9]{128}$/;
const MAX_BYTES = 4096;
const HOST_NEXT = 'review services; give host public ID to guest through an authorized channel; enter guest public ID within 60 seconds';
const GUEST_NEXT = 'authorize this public ID at the pinned host; paste its invitation within 60 seconds';
const REVIEW_NEXT = 'review pinned host, guest, services and expiry; enter approve followed by the exact digest';
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const record = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const key = value => typeof value === 'string' && KEY.test(value);

export class InvitationGateError extends Error {
  constructor(code) { super(code); this.name = 'InvitationGateError'; this.code = code; }
}

// Erase references in an event after delivery or while draining a failed agent.
// JSON strings and copies in V8/OS pipes cannot be guaranteed to be zeroized.
export function clearInvitationEvent(event) {
  if (!record(event)) return;
  for (const field of Object.keys(event)) delete event[field];
}

/**
 * One locally generated plan, one freshly spawned host/guest pair, one approval.
 * Public pins, the invitation and its review are private transient state. The
 * only outputs are passed directly to owned agent stdin by synchronous writers;
 * the gate never returns an envelope, review or reusable approval to its caller.
 */
export function createInvitationGate({ plan, planHash }) {
  return invitationGate(plan, planHash, false);
}

export function createFragmentedInvitationGate({ plan, planHash }) {
  return invitationGate(plan, planHash, true);
}

function invitationGate(plan, planHash, fragmented) {
  // Snapshot the caller's own compiled plan; a later mutation cannot change the
  // expected grants. Exact peer/event checks below fail closed on extra fields.
  const ownPlan = structuredClone(plan);
  let phase = 'host'; let host; let guest; let notBefore; let review;
  let issued; let expires; let monotonicDeadline;
  const createdAt = Date.now();
  const preparations = [];
  let hostReady = false;
  const clear = () => {
    host = undefined; guest = undefined; notBefore = undefined; review = undefined;
    issued = undefined; expires = undefined; monotonicDeadline = undefined;
    preparations.length = 0;
  };
  const reject = (code = 'game_tunnel_approval_mismatch') => {
    phase = 'closed'; clear(); throw new InvitationGateError(code);
  };
  const requirePhase = expected => { if (phase !== expected) reject(); };
  const preparation = event => {
    const end = event.preparationExpiresAtUnixMs; const now = Date.now();
    // A real local Rust/Node observation differed by +1 ms. Accept that one
    // millisecond only in the reported preparation timestamp, never in its
    // effective lifetime. Signed invitation times retain exact validation.
    const localEnd = now + PREPARATION_SECONDS * 1000;
    if (!Number.isSafeInteger(end) || now < createdAt || end <= now
      || end > localEnd + 1) reject('game_tunnel_invitation_expired');
    const boundedEnd = Math.min(end, localEnd);
    preparations.push({ end: boundedEnd, monotonic: performance.now() + (boundedEnd - now) });
    return { profile: FRAGMENTED_PROFILE, preparationSeconds: PREPARATION_SECONDS, preparationExpiresAtUnixMs: end };
  };
  const preparationsCurrent = () => {
    if (Date.now() < createdAt || preparations.some(p => Date.now() >= p.end || performance.now() >= p.monotonic)) {
      reject('game_tunnel_invitation_expired');
    }
  };
  const current = () => {
    const now = Date.now();
    if (now < issued || now >= expires || performance.now() >= monotonicDeadline) reject('game_tunnel_invitation_expired');
  };
  const planPreview = {
    schemaKind: 'localLabServicePlan', schemaVersion: 1, experimental: true,
    hashAlgorithm: 'sha256', hash: planHash, before: { session: 'notStarted' },
    after: { bind: ownPlan.bind, durationSeconds: ownPlan.durationSeconds, services: ownPlan.services },
    filesChanged: [], relay: false, modConfigurationChanges: false,
    authorization: 'requiresExplicitHashAndPinnedGuest', rollback: 'revokeAndStopLabProcess',
    minecraft: 'notVerified', simpleVoiceChat: 'notVerified', plasmoVoice: 'notVerified',
    ...(fragmented ? { profile: FRAGMENTED_PROFILE, udpEnvelopeVersion: 2, maxFragmentFrameBytes: FRAGMENT_FRAME_BYTES } : {}),
  };
  if (fragmented) {
    const aux = ownPlan.services[1];
    if (!key(planHash) || ownPlan.schemaVersion !== 1 || ownPlan.services.length !== 2
      || ownPlan.services[0].serviceId !== 'game' || ownPlan.services[0].protocol !== 'tcp'
      || ownPlan.services[0].requiredCapability !== 'reliableStream'
      || aux.serviceId !== 'aux' || aux.protocol !== 'udp' || aux.revision !== 1
      || aux.requiredCapability !== 'fragmentedDatagram' || aux.maxDatagramBytes !== ORIGINAL_DATAGRAM_BYTES
      || !['127.0.0.1:43002', '127.0.0.1:43003'].includes(aux.endpoint)
      || !isDeepStrictEqual(aux.udpReturnPolicy, { type: 'authorizedDownstream', bytesPerSecond: 16384, datagramsPerSecond: 20 })) reject();
  }
  const grants = ownPlan.services.map(service => ({
    serviceId: service.serviceId, protocol: service.protocol, revision: service.revision,
    requiredCapability: service.requiredCapability, maxDatagramBytes: service.maxDatagramBytes,
    udpReturnPolicy: service.udpReturnPolicy,
  }));

  return Object.freeze({
    hostIdentity(event) {
      requirePhase('host');
      if (!record(event) || !key(event.peer) || !isDeepStrictEqual(event, {
        event: 'invite_host_identity', peer: event.peer, plan: planPreview,
        ...(fragmented ? preparation(event) : {}), next: fragmented ? FRAGMENTED_HOST_NEXT : HOST_NEXT,
      })) reject();
      host = event.peer; phase = 'guest';
      // Needed only to start the fresh guest pinned to this owned host process.
      return host;
    },
    guestIdentity(event) {
      requirePhase('guest');
      if (!record(event) || !key(event.peer) || event.peer === host || !isDeepStrictEqual(event, {
        event: 'guest_identity', peer: event.peer,
        ...(fragmented ? preparation(event) : {}), next: fragmented ? FRAGMENTED_GUEST_NEXT : GUEST_NEXT,
      })) reject('game_tunnel_identity_invalid');
      guest = event.peer; phase = 'peer';
    },
    authorizePeer(write) {
      requirePhase('peer');
      if (fragmented) preparationsCurrent();
      notBefore = Date.now(); phase = 'invitation';
      write(guest);
    },
    invitation(event, write) {
      requirePhase('invitation');
      if (fragmented) preparationsCurrent();
      if (!record(event) || typeof event.invitation !== 'string'
        || Buffer.byteLength(event.invitation, 'utf8') < 1
        || Buffer.byteLength(event.invitation, 'utf8') > MAX_BYTES
        || !key(event.digest) || !isDeepStrictEqual(event, {
          event: 'host_invitation', invitation: event.invitation, digest: event.digest, singleAdmission: true,
        }) || digest(event.invitation) !== event.digest) reject();
      let envelope;
      try { envelope = JSON.parse(event.invitation); } catch { reject(); }
      if (!record(envelope) || !record(envelope.body) || typeof envelope.signature !== 'string'
        || !SIGNATURE.test(envelope.signature) || !key(envelope.body.invitationId)) reject();
      const body = envelope.body;
      const now = Date.now();
      issued = body.issuedAtUnixMs; expires = body.expiresAtUnixMs;
      if (!Number.isSafeInteger(issued) || !Number.isSafeInteger(expires)
        || issued < notBefore || issued > now || expires <= now || expires <= issued
        || expires - issued > Math.min(120, ownPlan.durationSeconds) * 1000) reject('game_tunnel_invitation_expired');
      const expectedBody = {
        schemaVersion: 1, hostKey: host, guestKey: guest, sessionId: host,
        invitationId: body.invitationId, issuedAtUnixMs: issued, expiresAtUnixMs: expires,
        generation: 1, directAddress: ownPlan.bind, services: grants,
      };
      // Match Rust's serialized order as well as values. This rejects duplicates,
      // unknown fields, aliases, whitespace and alternate JSON escaping without
      // introducing another signature implementation.
      if (event.invitation !== JSON.stringify({ body: expectedBody, signature: envelope.signature })) reject();
      monotonicDeadline = performance.now() + (expires - now);
      review = {
        event: 'invitation_review', digest: event.digest,
        invitation: { ...expectedBody, experimental: true, invitationSha256: event.digest,
          singleUse: true, relay: false, accountIdentityVerified: false,
          ...(fragmented ? { profile: FRAGMENTED_PROFILE, udpEnvelopeVersion: 2, maxFragmentFrameBytes: FRAGMENT_FRAME_BYTES } : {}) },
        localTcp: '127.0.0.1:0', localUdp: grants.length === 2 ? '127.0.0.1:35554' : null,
        ...(fragmented ? { profile: FRAGMENTED_PROFILE, approvalExpiresAtUnixMs: expires } : {}),
        next: fragmented ? FRAGMENTED_REVIEW_NEXT : REVIEW_NEXT,
      };
      phase = 'review';
      write(event.invitation);
    },
    hostReady(event) {
      requirePhase('review'); current();
      if (!fragmented || hostReady || !isDeepStrictEqual(event, {
        event: 'host_ready', experimental: true, relay: false, profile: FRAGMENTED_PROFILE, udpEnvelopeVersion: 2,
        expiresAtUnixMs: expires, wholeSessionMaximumSeconds: 600,
        next: 'enter stop then press Enter to revoke and close; fresh invitation required after shutdown',
      })) reject();
      hostReady = true;
    },
    review(event) {
      requirePhase('review'); current();
      if ((fragmented && !hostReady) || !isDeepStrictEqual(event, review)) reject();
      phase = 'approval';
    },
    approve(write) {
      requirePhase('approval'); current();
      const approvedDigest = review.digest;
      phase = 'approved'; clear();
      write(`approve ${approvedDigest}`);
    },
    dispose() { phase = 'closed'; clear(); },
  });
}
