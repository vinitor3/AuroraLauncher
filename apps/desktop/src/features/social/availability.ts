import { socialSessionId } from "./connect-types";

export type SocialAvailability = {
  sessionId: string; state: "ready" | "unavailable" | "unknown"; validUntil: number;
  admittedGuests: number | null; connectedGuests: number | null; hostPlayerConnected: boolean | null; maxGuests: number | null;
};
const keys = ["sessionId", "state", "validForMs", "admittedGuests", "connectedGuests", "hostPlayerConnected", "maxGuests"];
export function socialAvailabilitiesFrom(value: unknown, requested: string[], started: number): SocialAvailability[] {
  if (!Array.isArray(value) || value.length !== requested.length || value.length > 8 || !Number.isFinite(started)) throw "connect_social_failed";
  const seen = new Set<string>();
  return value.map(item => {
    if (!item || typeof item !== "object" || Array.isArray(item) || Object.keys(item).length !== keys.length || keys.some(key => !Object.hasOwn(item, key))
      || !socialSessionId(item.sessionId) || !requested.includes(item.sessionId) || seen.has(item.sessionId)
      || !["ready", "unavailable", "unknown"].includes(item.state) || !Number.isSafeInteger(item.validForMs) || item.validForMs < 0 || item.validForMs > 3000) throw "connect_social_failed";
    seen.add(item.sessionId);
    if (item.state === "ready") {
      if (!Number.isInteger(item.maxGuests) || item.maxGuests < 1 || item.maxGuests > 8 || !Number.isInteger(item.admittedGuests) || item.admittedGuests < 0 || item.admittedGuests > item.maxGuests
        || !Number.isInteger(item.connectedGuests) || item.connectedGuests < 0 || item.connectedGuests > item.admittedGuests || typeof item.hostPlayerConnected !== "boolean"
        || (item.hostPlayerConnected && item.maxGuests > 7) || item.validForMs === 0) throw "connect_social_failed";
    } else if (item.admittedGuests !== null || item.connectedGuests !== null || item.hostPlayerConnected !== null || item.maxGuests !== null || (item.state === "unknown" && item.validForMs !== 0)) throw "connect_social_failed";
    return { sessionId: item.sessionId, state: item.state, validUntil: started + item.validForMs, admittedGuests: item.admittedGuests,
      connectedGuests: item.connectedGuests, hostPlayerConnected: item.hostPlayerConnected, maxGuests: item.maxGuests };
  });
}
