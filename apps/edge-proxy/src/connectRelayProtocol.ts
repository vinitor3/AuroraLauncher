import { blake3 } from "@noble/hashes/blake3.js";

// Wire compatibility: iroh-relay 1.3.0, protos/{handshake,relay,common}.rs.
// This carries the existing encrypted Iroh packets; it never opens destination sockets.
const challengeContext = new TextEncoder().encode("iroh-relay handshake v1 challenge signature");
export const RELAY_PROTOCOL = "iroh-relay-v2";
export const MAX_RELAY_FRAME = 65_537;
export const hex = (bytes: Uint8Array): string => Array.from(bytes, b => b.toString(16).padStart(2, "0")).join("");
export function peerBytes(value: string): Uint8Array {
  if (!/^[0-9a-f]{64}$/.test(value)) throw new Error("relay_invalid_peer");
  return Uint8Array.from(value.match(/../g)!, byte => Number.parseInt(byte, 16));
}
export function createRelayChallenge(): Uint8Array {
  const frame = new Uint8Array(17);
  crypto.getRandomValues(frame.subarray(1));
  return frame;
}
export async function verifyRelayAuthentication(challenge: Uint8Array, frame: Uint8Array, expectedPeer: string): Promise<boolean> {
  // PublicKey is a fixed 32-byte postcard array; serde_bytes Signature has a 64 length prefix.
  if (challenge.length !== 17 || challenge[0] !== 0 || frame.length !== 98 || frame[0] !== 1 || frame[33] !== 64) return false;
  if (hex(frame.subarray(1, 33)) !== expectedPeer) return false;
  try {
    const key = await crypto.subtle.importKey("raw", peerBytes(expectedPeer), "Ed25519", false, ["verify"]);
    const message = blake3(challenge.subarray(1), { context: challengeContext, dkLen: 32 });
    return await crypto.subtle.verify("Ed25519", key, frame.slice(34), message);
  } catch { return false; }
}

export type RelayFrame = { kind: "ping"; payload: Uint8Array } | { kind: "pong" } | { kind: "datagram"; destination: string; forwarded: Uint8Array };
export function parseRelayFrame(frame: Uint8Array, sender: string): RelayFrame {
  if (!frame.length || frame.length > MAX_RELAY_FRAME) throw new Error("relay_invalid_frame");
  const tag = frame[0];
  if (tag === 9 && frame.length === 9) return { kind: "ping", payload: Uint8Array.of(10, ...frame.subarray(1)) };
  if (tag === 10 && frame.length === 9) return { kind: "pong" };
  if ((tag !== 4 && tag !== 5) || frame.length < (tag === 5 ? 37 : 35)) throw new Error("relay_invalid_frame");
  // ECN encodes absent=0 or the two low IP bits (1..3), never arbitrary flags.
  if (frame[33] > 3) throw new Error("relay_invalid_frame");
  if (tag === 5) {
    const segment = (frame[34] << 8) | frame[35];
    // Iroh accepts the final segment being shorter than the declared GSO size.
    if (segment === 0) throw new Error("relay_invalid_frame");
  }
  const destination = hex(frame.subarray(1, 33));
  const forwarded = frame.slice();
  forwarded[0] = tag + 2;
  forwarded.set(peerBytes(sender), 1);
  return { kind: "datagram", destination, forwarded };
}

export type RelayBudget = {
  startMs: number; spent: number; windowMs: number; windowBytes: number; frames: number; rateLimited: boolean;
};
export const relayWindow = (now: number): number => Math.floor(now / 1000) * 1000;
export const newRelayBudget = (now: number): RelayBudget => ({
  startMs: now, spent: 0, windowMs: relayWindow(now), windowBytes: 0, frames: 0, rateLimited: false,
});
export function debitRelayBudget(budget: RelayBudget, bytes: number, now: number, deadline: number): RelayBudget {
  const windowMs = relayWindow(now);
  if (!Number.isSafeInteger(bytes) || bytes < 1 || bytes > MAX_RELAY_FRAME
    || now < budget.startMs || windowMs < budget.windowMs || now >= deadline) throw new Error("relay_expired");
  const next = { ...budget };
  if (windowMs > next.windowMs) {
    next.windowMs = windowMs;
    next.windowBytes = 0;
    next.frames = 0;
  }
  next.frames++;
  if (next.spent + bytes > 268_435_456 || next.frames > 1024) throw new Error("relay_budget_exhausted");
  next.rateLimited = next.windowBytes + bytes > 1_048_576;
  if (!next.rateLimited) {
    next.spent += bytes;
    next.windowBytes += bytes;
  }
  return next;
}
