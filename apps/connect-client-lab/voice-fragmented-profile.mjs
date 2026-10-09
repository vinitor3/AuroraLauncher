// Explicit local laboratory profile. It is not selected by invitation contents,
// an environment variable, a user supplied executable or a legacy approval.
import { isDeepStrictEqual } from 'node:util';

export const FRAGMENTED_PROFILE = 'fragmentedV2';
export const ORIGINAL_DATAGRAM_BYTES = 4096;
export const FRAGMENT_FRAME_BYTES = 1264;
export const PREPARATION_SECONDS = 600;
export const FRAGMENTED_HOST_NEXT = 'review services; give host public ID to guest through an authorized channel; enter guest public ID then press Enter before preparation expiry; if the native program exited do not paste into PowerShell';
export const FRAGMENTED_GUEST_NEXT = 'authorize this public ID at the pinned host; paste the invitation then press Enter before preparation expiry; keep the same native process; if it exited do not paste into PowerShell';
export const FRAGMENTED_REVIEW_NEXT = 'review pinned IDs, services and exact expiry; enter approve followed by exact digest then press Enter; a line without Enter is not approval and expiry is never renewed';

// Immutable owned copy from reviewed source commit078a3b0, 24,401,920 bytes.
// This is an artifact identity, not a signing/trust assertion or caller option.
const ARTIFACT_PATH = '.aurora/connect-validation/voice53-reviewed/aurora-connect-lab.exe';
const ARTIFACT_SHA256 = 'adcc23b75c5b4222fba582c18ccf4147c46e984197e9724e9dae1a4cd26b0423';

export function fragmentedVoiceContract() {
  if (!/^[a-f0-9]{64}$/.test(ARTIFACT_SHA256 ?? '')) throw new Error('game_tunnel_fragmented_artifact_pending');
  return Object.freeze({ profile: FRAGMENTED_PROFILE, invitationSchemaVersion: 1, udpEnvelopeVersion: 2,
    executable: ARTIFACT_PATH, executableSha256: ARTIFACT_SHA256,
    hostCommand: 'host-invite-plan-fragmented', guestCommand: 'guest-invite-fragmented',
    requiredCapability: 'fragmentedDatagram', approvedOriginalDatagramBytes: ORIGINAL_DATAGRAM_BYTES,
    maxFragmentFrameBytes: FRAGMENT_FRAME_BYTES, relay: false, audio: false });
}

// Strict V2 terminal events remain independently testable without subprocesses.
export function fragmentedReady(event) {
  const match = typeof event?.tcp === 'string' && /^127\.0\.0\.1:([1-9][0-9]{0,4})$/.exec(event.tcp);
  if (!match || Number(match[1]) > 65535 || !isDeepStrictEqual(event, {
    event: 'guest_ready', tcp: event.tcp, udp: '127.0.0.1:35554', profile: FRAGMENTED_PROFILE,
    udpEnvelopeVersion: 2, approvedOriginalDatagramBytes: ORIGINAL_DATAGRAM_BYTES,
    transportOriginalCapacity: ORIGINAL_DATAGRAM_BYTES, associationCeilingNegotiated: true, capacityScope: 'initialAssociation',
    maxFragmentFrameBytes: FRAGMENT_FRAME_BYTES, experimental: true, relay: false,
    next: 'enter stop then press Enter to close; invitation whole-session expiry is unchanged',
  })) throw new Error('game_tunnel_mode_invalid');
  return Object.freeze({ bridgePort: Number(match[1]), voicePort: 35554, profile: FRAGMENTED_PROFILE,
    approvedOriginalDatagramBytes: ORIGINAL_DATAGRAM_BYTES, transportOriginalCapacity: ORIGINAL_DATAGRAM_BYTES,
    associationCeilingNegotiated: true, capacityScope: 'initialAssociation', maxFragmentFrameBytes: FRAGMENT_FRAME_BYTES });
}

export function fragmentedSnapshot(event) {
  const tx = event?.datagramFramesTxSinceReady; const rx = event?.datagramFramesRxSinceReady;
  if (!Number.isSafeInteger(tx) || tx < 0 || !Number.isSafeInteger(rx) || rx < 0
    || !isDeepStrictEqual(event, { event: 'fragmented_transport_snapshot', experimental: true,
      profile: FRAGMENTED_PROFILE, datagramFramesTxSinceReady: tx, datagramFramesRxSinceReady: rx,
      evidence: 'transport_counters_only_not_minecraft_or_voice' })) throw new Error('game_tunnel_mode_invalid');
  return Object.freeze({ ...event });
}

// This checks ordering only; the invitation gate and readiness checks still
// validate every event payload. A snapshot never substitutes for process exit
// and guest_stopped, both of which the caller must await before success.
export function createFragmentedEventSequence(kind) {
  if (!['host', 'guest'].includes(kind)) throw new Error('game_tunnel_event_unexpected');
  const names = kind === 'host'
    ? ['invite_host_identity', 'host_invitation', 'host_ready', 'host_stopped']
    : ['guest_identity', 'invitation_review', 'guest_ready', 'fragmented_transport_snapshot', 'guest_stopped'];
  let position = 0; let failed = false; let transport;
  return Object.freeze({
    accept(event, closing) {
      try {
        if (failed || position === names.length || event?.event !== names[position]
          || ((event.event === 'fragmented_transport_snapshot' || event.event === `${kind}_stopped`) && closing !== true)) {
          throw new Error('game_tunnel_event_unexpected');
        }
        if (event.event === 'fragmented_transport_snapshot') transport = fragmentedSnapshot(event);
        if (event.event === `${kind}_stopped` && !isDeepStrictEqual(event, { event: `${kind}_stopped` })) {
          throw new Error('game_tunnel_event_unexpected');
        }
        position++;
      } catch (error) { failed = true; transport = undefined; throw error; }
    },
    get finished() { return !failed && position === names.length; },
    get transport() { return transport; },
  });
}
