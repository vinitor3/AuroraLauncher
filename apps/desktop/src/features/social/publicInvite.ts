import { socialSessionId } from "./connect-types";

export const publicInvitePrefix = "https://aurora-api.aurora-edge-proxy.workers.dev/join/";
const codePrefix = "aurora-v1:";

export function publicInviteId(input: string): string | null {
  const text = input.trim();
  const prefix = [publicInvitePrefix, codePrefix].find(value => text.startsWith(value));
  const id = prefix && text.slice(prefix.length);
  return socialSessionId(id) ? id : null;
}
