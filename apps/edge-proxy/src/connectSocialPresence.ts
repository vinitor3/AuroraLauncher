/** Display-only observations; never an admission, renewal or world-player count. */
import type { SocialAdmissionState } from "./connectSocialAdmission.ts";

export function projectSocialPresence(sessionId: string, admission: SocialAdmissionState | undefined, now: number, usable: boolean) {
  const empty = { schemaVersion: 1, sessionId, state: "unavailable" as "ready" | "unavailable" | "unknown", serverNowUnixMs: now,
    checkedAtUnixMs: null as number | null, expiresAtUnixMs: null as number | null, admittedGuests: null as number | null,
    connectedGuests: null as number | null, hostPlayerConnected: null as boolean | null, maxGuests: null as number | null };
  if (!usable || !admission) return empty;
  const policy = admission.policies.find(([, value]) => value.body.sessionId === sessionId && value.state !== "terminal" && value.state !== "registering"
    && value.body.expiresAtUnixMs > now && value.checkedAt !== undefined && value.checkedAt <= now && value.checkedAt + 3_000 > now)?.[1];
  if (!policy) return empty;
  const transport = policy.transport;
  if (!transport || transport.checkedAt > now || transport.checkedAt + 3_000 <= now) return { ...empty, state: "unknown" as const };
  const grants = admission.grants.filter(([, value]) => value.policyId === policy.body.policyId && !value.terminal
    && value.body.sessionExpiresAtUnixMs > now && value.lastLease && value.lastLease.issuedAtUnixMs <= now && value.lastLease.expiresAtUnixMs > now);
  const connected = new Set(transport.connectedGrantIds);
  // One expired guest is removed from the count; it cannot retire other peers.
  return { ...empty, state: "ready" as const, checkedAtUnixMs: transport.checkedAt,
    expiresAtUnixMs: Math.min(policy.body.expiresAtUnixMs, policy.checkedAt! + 3_000, transport.checkedAt + 3_000,
      ...grants.map(([, value]) => value.lastLease!.expiresAtUnixMs)),
    admittedGuests: grants.length, connectedGuests: grants.filter(([id]) => connected.has(id)).length,
    hostPlayerConnected: policy.body.hostPlayer === true && transport.hostPlayerConnected, maxGuests: policy.body.maxGuests };
}
