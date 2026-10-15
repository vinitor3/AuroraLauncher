import { fragmentedContractFrom, isDirectIpv4Address, isLoopbackUdpTarget, SESSION_FRAGMENTED_PROFILE, type FragmentedUdpContract, type FragmentedUdpReview } from "./fragmented-types";
import type { ConnectMode, ConnectSessionTiming } from "./types";

export const GROUP_KIND = "staticSessionFragmentedHost" as const;
export type GroupCapability = {
  profile: typeof SESSION_FRAGMENTED_PROFILE; invitationSchemaVersion: 3;
  minParticipants: 2; maxParticipants: 8; maxPreparationSeconds: 600; maxReviewSeconds: 60;
  maxAdmissionSeconds: 600; maxSessionSeconds: 3600; ownedGameTcpDiscovery: boolean;
};
export type PendingGroupSource = { kind: typeof GROUP_KIND; mode: ConnectMode };
export type GroupPreparation = { operationId: string; generation: number; publicId: string; expiresAtMs: number; kind: typeof GROUP_KIND };
export type GroupParticipant = {
  participantId: string; guestPublicId: string; invitationDigest: string; sessionTiming: ConnectSessionTiming;
  consumed: boolean; transportActive: boolean; revoked: boolean; admissionValid: boolean; sessionValid: boolean;
};
export type GroupReview = {
  operationId: string; generation: number; digest: string; kind: typeof GROUP_KIND; mode: ConnectMode;
  hostPublicId: string; directAddress: string; gameTarget: string;
  fragmentedUdp: FragmentedUdpReview; participants: GroupParticipant[]; expiresAtMs: number;
} & ({ targetSource: "manual" } | { targetSource: "ownedInstance"; instanceId: string; runId: string });
export type HostGroup = {
  kind: typeof GROUP_KIND; profile: typeof SESSION_FRAGMENTED_PROFILE; invitationSchemaVersion: 3;
  mode: ConnectMode; hostPublicId: string; preparation: GroupPreparation | null;
  review: GroupReview | null; participants: GroupParticipant[];
};
export type GroupInvitationExport = {
  operationId: string; participantId: string; invitationDigest: string; invitation: string;
  hostPublicId: string; expiresAtMs: number; sessionTiming: ConnectSessionTiming; fragmentedUdp: FragmentedUdpContract;
};
type RecordValue = Record<string, unknown>;
const bad = (): never => { throw new Error("Invalid Connect group projection"); };
const record = (value: unknown): value is RecordValue => !!value && typeof value === "object" && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is RecordValue => record(value)
  && Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
const integer = (value: unknown): value is number => Number.isSafeInteger(value) && (value as number) >= 0;
const hash = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const id = (value: unknown): value is string => typeof value === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value);
const text = (value: unknown): value is string => typeof value === "string" && value.length > 0 && value.length <= 256 && !/[\u0000-\u001f\u007f]/u.test(value);
const mode = (value: unknown): value is ConnectMode => ["loopback", "externalIpv6", "directIpv4"].includes(value as string);
// Parse only a literal projection; URL parsing performs no network request.
// Mirror the native conservative initial-address policy, not reachability.
function externalIpv6Address(value: string): boolean {
  const match = /^\[([0-9a-f:]+)\]:([1-9][0-9]{0,4})$/.exec(value);
  if (!match || Number(match[2]) > 65535) return false;
  try { if (new URL(`http://${value}/`).hostname !== `[${match[1]}]`) return false; }
  catch { return false; }
  const halves = match[1].split("::");
  const left = halves[0].split(":").filter(Boolean);
  const right = (halves[1] ?? "").split(":").filter(Boolean);
  const parts = halves.length === 2 ? [...left, ...Array<string>(8 - left.length - right.length).fill("0"), ...right] : left;
  const s = parts.map(part => Number.parseInt(part, 16));
  return s.length === 8 && (s[0] & 0xe000) === 0x2000
    && !(s[0] === 0x2001 && (s[1] & 0xfe00) === 0) && !(s[0] === 0x2001 && s[1] === 0x0db8)
    && s[0] !== 0x2002 && !(s[0] === 0x2620 && s[1] === 0x004f && s[2] === 0x8000)
    && !(s[0] === 0x3fff && (s[1] & 0xf000) === 0) && !([0, 0x0200].includes(s[4]) && s[5] === 0x5efe);
}
const PARTICIPANT_KEYS = ["participantId", "guestPublicId", "invitationDigest", "sessionTiming", "consumed", "transportActive", "revoked", "admissionValid", "sessionValid"];
function timingFrom(value: unknown): ConnectSessionTiming {
  if (!exact(value, ["profile", "schemaVersion", "admissionSeconds", "sessionSeconds", "admissionExpiresAtMs", "sessionExpiresAtMs"])
    || value.profile !== SESSION_FRAGMENTED_PROFILE || value.schemaVersion !== 3
    || !integer(value.admissionSeconds) || value.admissionSeconds < 1 || value.admissionSeconds > 600
    || !integer(value.sessionSeconds) || value.sessionSeconds < value.admissionSeconds || value.sessionSeconds > 3600
    || !integer(value.admissionExpiresAtMs) || !integer(value.sessionExpiresAtMs)
    || value.admissionExpiresAtMs < value.admissionSeconds * 1000
    || value.sessionExpiresAtMs - value.admissionExpiresAtMs !== (value.sessionSeconds - value.admissionSeconds) * 1000) return bad();
  return value as ConnectSessionTiming;
}
export function sameGroupParticipant(left: GroupParticipant, right: GroupParticipant): boolean {
  return left.participantId === right.participantId && left.guestPublicId === right.guestPublicId && left.invitationDigest === right.invitationDigest
    && sameGroupTiming(left.sessionTiming, right.sessionTiming);
}
export function sameGroupTiming(a: ConnectSessionTiming, b: ConnectSessionTiming): boolean {
  return a.profile === b.profile && a.schemaVersion === b.schemaVersion && a.admissionSeconds === b.admissionSeconds
    && a.sessionSeconds === b.sessionSeconds && a.admissionExpiresAtMs === b.admissionExpiresAtMs && a.sessionExpiresAtMs === b.sessionExpiresAtMs;
}
export function groupCapabilityFrom(value: unknown): GroupCapability {
  if (!exact(value, ["profile", "invitationSchemaVersion", "minParticipants", "maxParticipants", "maxPreparationSeconds", "maxReviewSeconds", "maxAdmissionSeconds", "maxSessionSeconds", "ownedGameTcpDiscovery"])
    || value.profile !== SESSION_FRAGMENTED_PROFILE || value.invitationSchemaVersion !== 3 || value.minParticipants !== 2
    || value.maxParticipants !== 8 || value.maxPreparationSeconds !== 600 || value.maxReviewSeconds !== 60
    || value.maxAdmissionSeconds !== 600 || value.maxSessionSeconds !== 3600 || typeof value.ownedGameTcpDiscovery !== "boolean") return bad();
  return value as GroupCapability;
}
export function pendingGroupSourceFieldsFrom(value: RecordValue): PendingGroupSource {
  const pending = value.pendingGroupSource;
  if (!exact(value, ["available", "operationId", "generation", "role", "phase", "transportState", "statusState", "tcpAddress", "expiresAtMs",
    "statusSummary", "errorCode", "worldState", "simpleVoiceChatState", "plasmoVoiceState", "relayEnabled", "pathState", "preparation", "review", "cleanupState", "pendingGroupSource"])
    || !exact(pending, ["kind", "mode"]) || pending.kind !== GROUP_KIND || !mode(pending.mode)
    || value.available !== true || !id(value.operationId) || !integer(value.generation) || value.generation === 0
    || value.role !== "host" || value.phase !== "hostPreparing" || value.transportState !== "notStarted" || value.statusState !== "notChecked"
    || value.tcpAddress !== null || value.statusSummary !== null || value.errorCode !== null || value.preparation !== null || value.review !== null
    || value.cleanupState !== "pending" || !integer(value.expiresAtMs) || value.expiresAtMs === 0) return bad();
  return pending as PendingGroupSource;
}
export function groupPreparationFrom(value: unknown): GroupPreparation {
  if (!exact(value, ["operationId", "generation", "publicId", "expiresAtMs", "kind"]) || value.kind !== GROUP_KIND
    || !id(value.operationId) || !integer(value.generation) || !hash(value.publicId) || !integer(value.expiresAtMs) || value.expiresAtMs === 0) return bad();
  return value as GroupPreparation;
}
function participantsFrom(value: unknown, hostId: string, allowEmpty = false): GroupParticipant[] {
  if (!Array.isArray(value) || (!(allowEmpty && value.length === 0) && (value.length < 2 || value.length > 8))) return bad();
  const ids = new Set<string>(), peers = new Set<string>(), digests = new Set<string>();
  for (const item of value) {
    if (!exact(item, PARTICIPANT_KEYS) || !id(item.participantId) || !hash(item.guestPublicId) || item.guestPublicId === hostId || !hash(item.invitationDigest)
      || !["consumed", "transportActive", "revoked", "admissionValid", "sessionValid"].every(key => typeof item[key] === "boolean")
      || ids.has(item.participantId) || peers.has(item.guestPublicId) || digests.has(item.invitationDigest)
      || (item.revoked && (item.admissionValid || item.sessionValid || item.transportActive))
      || (item.admissionValid && !item.sessionValid) || (item.transportActive && (!item.consumed || !item.sessionValid))) return bad();
    timingFrom(item.sessionTiming);
    ids.add(item.participantId); peers.add(item.guestPublicId); digests.add(item.invitationDigest);
  }
  return value as GroupParticipant[];
}
export function groupReviewFrom(value: unknown): GroupReview {
  if (!record(value)) return bad();
  const sourceKeys = value.targetSource === "ownedInstance" ? ["instanceId", "runId"] : [];
  if (!exact(value, ["operationId", "generation", "digest", "kind", "mode", "hostPublicId", "directAddress", "gameTarget", "targetSource", "fragmentedUdp", "participants", "expiresAtMs", ...sourceKeys])
    || !id(value.operationId) || !integer(value.generation) || !hash(value.digest) || value.kind !== GROUP_KIND || !mode(value.mode)
    || !hash(value.hostPublicId) || typeof value.directAddress !== "string" || value.directAddress.length > 128
    || (value.mode === "loopback" && !isLoopbackUdpTarget(value.directAddress))
    || (value.mode === "directIpv4" && !isDirectIpv4Address(value.directAddress))
    || (value.mode === "externalIpv6" && !externalIpv6Address(value.directAddress))
    || !isLoopbackUdpTarget(value.gameTarget) || !["manual", "ownedInstance"].includes(value.targetSource as string)
    || (value.targetSource === "ownedInstance" && (!text(value.instanceId) || !text(value.runId)))
    || !integer(value.expiresAtMs) || value.expiresAtMs === 0
    || !record(value.fragmentedUdp) || !isLoopbackUdpTarget(value.fragmentedUdp.udpTarget)) return bad();
  const { udpTarget: _target, ...contract } = value.fragmentedUdp;
  fragmentedContractFrom(contract, SESSION_FRAGMENTED_PROFILE);
  const participants = participantsFrom(value.participants, value.hostPublicId);
  if (participants.some(item => item.consumed || item.transportActive || item.revoked || !item.admissionValid || !item.sessionValid
    || (value.expiresAtMs as number) > item.sessionTiming.admissionExpiresAtMs)) return bad();
  return value as GroupReview;
}
export function groupSnapshotFieldsFrom(value: RecordValue): HostGroup {
  const group = value.hostGroup;
  if (!exact(group, ["kind", "profile", "invitationSchemaVersion", "mode", "hostPublicId", "preparation", "review", "participants"])
    || group.kind !== GROUP_KIND || group.profile !== SESSION_FRAGMENTED_PROFILE || group.invitationSchemaVersion !== 3
    || !mode(group.mode) || !hash(group.hostPublicId) || value.available !== true || value.role !== "host" || !id(value.operationId) || value.pendingGroupSource !== undefined
    || !["hostPrepared", "reviewed", "starting", "hostListening", "closing", "closed", "failed"].includes(value.phase as string)
    || value.preparation !== null || value.review !== null || value.tcpAddress !== null || value.statusSummary !== null || value.statusState !== "notChecked"
    || ["invitationProfile", "sessionTiming", "fragmentedUdp", "udpState", "udpAddress", "udpErrorCode", "udpOriginalCapacity", "udpCapacityScope", "udpRecovery", "udpTarget"]
      .some(key => value[key] !== undefined)) return bad();
  const participants = participantsFrom(group.participants, group.hostPublicId, ["hostPrepared", "closing", "closed", "failed"].includes(value.phase as string));
  if (participants.some(item => item.transportActive && value.phase !== "hostListening")) return bad();
  if (["closing", "closed", "failed"].includes(value.phase as string)
    && participants.some(item => item.admissionValid || item.sessionValid || item.transportActive)) return bad();
  if (value.phase === "hostPrepared") {
    const preparation = groupPreparationFrom(group.preparation);
    if (participants.length !== 0 || group.review !== null || preparation.operationId !== value.operationId
      || preparation.generation !== value.generation || preparation.publicId !== group.hostPublicId || value.expiresAtMs !== preparation.expiresAtMs) return bad();
  } else if (group.preparation !== null) return bad();
  if (value.phase === "reviewed") {
    const review = groupReviewFrom(group.review);
    if (review.operationId !== value.operationId || review.generation !== value.generation || review.mode !== group.mode
      || review.hostPublicId !== group.hostPublicId || value.expiresAtMs !== review.expiresAtMs
      || review.participants.length !== participants.length || review.participants.some((item, index) => !sameGroupParticipant(item, participants[index]))
      || participants.some(item => item.consumed || item.transportActive || item.revoked || !item.admissionValid || !item.sessionValid)) return bad();
  } else if (group.review !== null) return bad();
  if (["hostListening", "closing", "closed", "failed"].includes(value.phase as string) && value.expiresAtMs !== null) return bad();
  if (["hostPrepared", "reviewed"].includes(value.phase as string) && value.transportState !== "notStarted") return bad();
  if (value.phase === "hostListening" && value.transportState !== "listening") return bad();
  if (value.phase === "closed" && value.transportState !== "closed") return bad();
  if (value.phase === "failed" && (value.cleanupState === "failed"
    ? !["notStarted", "listening", "closed"].includes(value.transportState as string)
    : !["pending", "complete"].includes(value.cleanupState as string) || value.transportState !== "closed")) return bad();
  return group as HostGroup;
}
export function groupExportFrom(value: unknown): GroupInvitationExport {
  if (!exact(value, ["operationId", "participantId", "invitationDigest", "invitation", "hostPublicId", "expiresAtMs", "sessionTiming", "fragmentedUdp"])
    || !id(value.operationId) || !id(value.participantId) || !hash(value.invitationDigest) || !hash(value.hostPublicId)
    || typeof value.invitation !== "string" || value.invitation.length === 0 || new TextEncoder().encode(value.invitation).length > 4096
    || !integer(value.expiresAtMs)) return bad();
  const timing = timingFrom(value.sessionTiming);
  if (value.expiresAtMs !== timing.admissionExpiresAtMs) return bad();
  fragmentedContractFrom(value.fragmentedUdp, SESSION_FRAGMENTED_PROFILE);
  return value as GroupInvitationExport;
}
