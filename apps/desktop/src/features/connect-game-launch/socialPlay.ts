export type SocialPlayTarget = { operationId: string; sessionId: string };
export type SocialPlayRequest = SocialPlayTarget & { contextKey: string; claimCopyLookup(): boolean };

/** One explicit join or open-game gesture, scoped to this connection and instance selection. */
export function socialPlayRequest(target: SocialPlayTarget, contextKey: string): SocialPlayRequest {
  let available = true;
  return { ...target, contextKey, claimCopyLookup() {
    if (!available) return false;
    available = false;
    return true;
  } };
}
