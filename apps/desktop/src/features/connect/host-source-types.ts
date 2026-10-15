import type { ConnectMode, ConnectSnapshot } from "./types";

export type HostSourceProfile = "legacy" | "sessionGameTcp" | "sessionFragmentedGameUdp";
export type PendingHostSource = { kind: "ownedGameTcp"; mode: ConnectMode; profile: HostSourceProfile };
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
const exact = (value: unknown, keys: string[]): value is Record<string, unknown> => record(value)
  && Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
const positive = (value: unknown) => Number.isSafeInteger(value) && (value as number) > 0;
const invalid = (): never => { throw new Error("Invalid pending host source"); };

export function pendingHostSourceFieldsFrom(value: Record<string, unknown>): PendingHostSource {
  const pending = value.pendingHostSource;
  if (!exact(pending, ["kind", "mode", "profile"]) || pending.kind !== "ownedGameTcp"
    || !["loopback", "externalIpv6", "directIpv4"].includes(pending.mode as string)
    || !["legacy", "sessionGameTcp", "sessionFragmentedGameUdp"].includes(pending.profile as string)) return invalid();
  const session = pending.profile !== "legacy";
  if (!exact(value, ["available", "operationId", "generation", "role", "phase", "transportState", "statusState", "tcpAddress", "expiresAtMs",
    "statusSummary", "errorCode", "worldState", "simpleVoiceChatState", "plasmoVoiceState", "relayEnabled", "pathState", "preparation", "review", "cleanupState", "pendingHostSource",
    ...(session ? ["invitationProfile"] : [])])
    || value.available !== true || typeof value.operationId !== "string" || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value.operationId)
    || !positive(value.generation) || value.role !== "host" || value.phase !== "hostPreparing"
    || value.transportState !== "notStarted" || value.statusState !== "notChecked" || value.tcpAddress !== null || value.statusSummary !== null
    || value.errorCode !== null || value.preparation !== null || value.review !== null || value.cleanupState !== "pending" || !positive(value.expiresAtMs)
    || (session && value.invitationProfile !== pending.profile)) return invalid();
  return pending as PendingHostSource;
}

// Only exact native error strings are definitive replies. IPC exceptions,
// malformed projections and arbitrary objects never establish rejection.
export function isControlledSourceRejection(cause: unknown): boolean {
  return typeof cause === "string" && [
    "connect_disabled", "connect_owner_mismatch", "connect_invalid_request", "connect_busy", "connect_cancelled", "connect_internal",
    "connect_plan_expired", "connect_source_changed", "connect_stale_operation", "discovery_unavailable", "discovery_invalid_request",
    "discovery_process_changed", "discovery_listener_changed", "discovery_inventory_limit", "discovery_stale_review",
    "discovery_owner_mismatch", "discovery_busy", "discovery_internal",
  ].includes(cause);
}

// Call only with a fresh snapshot whose invoke began after the definitive
// rejection. The manager never removes/reverts a reserved Attempt, so a new
// reservation cannot produce the same UUID and generation as this baseline.
export function provesNoSourceReservation(next: ConnectSnapshot, baseline: ConnectSnapshot | undefined): boolean {
  if (!baseline || next.operationId !== baseline.operationId || next.generation !== baseline.generation) return false;
  const released = (value: ConnectSnapshot) => value.phase === "idle"
    ? value.operationId === null && value.role === null && value.transportState === "notStarted"
      && ["notStarted", "complete"].includes(value.cleanupState) && value.preparation === null && value.review === null
    : ["closed", "failed"].includes(value.phase) && value.operationId !== null && value.transportState === "closed"
      && value.cleanupState === "complete" && value.preparation === null && value.review === null;
  return released(baseline) && released(next);
}
