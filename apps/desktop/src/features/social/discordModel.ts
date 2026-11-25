import type { SocialConnectState } from "./connect-controller";
import { socialSessionId } from "./connect-types";

export type DiscordActivity = { playing: boolean; party: { sessionId: string; current: number; max: number; expiresAtMs: number } | null };
export type DiscordSettings = { enabled: boolean; applicationId: string };
export type DiscordControls = {
  settings: DiscordSettings; defaultId: string; status: string; sessionId: string | null;
  configure(settings: DiscordSettings): void; dismiss(): void;
};
export function validDiscordId(id: string): boolean {
  return /^[1-9][0-9]{16,19}$/.test(id) && BigInt(id) <= 18446744073709551615n;
}
export function discordActivity(playing: boolean, state: SocialConnectState | undefined, now: number): DiscordActivity {
  const snapshot = state?.snapshot, social = snapshot?.social;
  const ordinary = { playing, party: null };
  if (!state?.available || state.error || !snapshot?.operationId || snapshot.errorCode || snapshot.role !== "host"
    || snapshot.cleanupState !== "notStarted" || ["closed", "closing", "failed"].includes(snapshot.phase)
    || social?.visibility !== "public" || social.role !== "host" || !socialSessionId(social.sessionId)
    || !["waitingGuest", "connected"].includes(social.stage) || !social.expiresAtMs || social.expiresAtMs <= now) return ordinary;
  // Older single-guest hosts expose no participant roster. Only waitingGuest guarantees an open seat.
  if (!social.participants && social.stage !== "waitingGuest") return ordinary;
  const current = 1 + (social.participants?.filter(p => p.state !== "disconnected").length ?? 0);
  const max = 1 + (social.maxGuests ?? 1);
  return current < max ? { playing, party: { sessionId: social.sessionId, current, max, expiresAtMs: social.expiresAtMs } } : ordinary;
}
