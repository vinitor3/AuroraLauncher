import test from "node:test";
import assert from "node:assert/strict";
import { blake3 } from "@noble/hashes/blake3.js";
import { createRelayChallenge, verifyRelayAuthentication, hex, peerBytes, parseRelayFrame, newRelayBudget, debitRelayBudget, MAX_RELAY_FRAME } from "./connectRelayProtocol.ts";

test("Iroh challenge binds actual Ed25519 possession to the permitted peer and one challenge", async () => {
  const pair = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]);
  const publicKey = new Uint8Array(await crypto.subtle.exportKey("raw", pair.publicKey));
  const challenge = createRelayChallenge();
  const signed = blake3(challenge.subarray(1), { context: new TextEncoder().encode("iroh-relay handshake v1 challenge signature") });
  const signature = new Uint8Array(await crypto.subtle.sign("Ed25519", pair.privateKey, signed));
  const frame = Uint8Array.of(1, ...publicKey, 64, ...signature);
  assert.equal(await verifyRelayAuthentication(challenge, frame, hex(publicKey)), true);
  assert.equal(await verifyRelayAuthentication(createRelayChallenge(), frame, hex(publicKey)), false);
  assert.equal(await verifyRelayAuthentication(challenge, frame, "ff".repeat(32)), false);
  assert.equal(await verifyRelayAuthentication(challenge, Uint8Array.of(...frame, 0), hex(publicKey)), false);
  frame[33] = 63;
  assert.equal(await verifyRelayAuthentication(challenge, frame, hex(publicKey)), false);
});
test("relay substitutes authenticated sender and preserves opaque single/batch payload", () => {
  const host = "11".repeat(32), guest = "22".repeat(32);
  for (const tag of [4, 5]) {
    const frame = Uint8Array.of(tag, ...peerBytes(host), 0, ...(tag === 5 ? [0, 3] : []), 9, 8, 7, 6);
    const parsed = parseRelayFrame(frame, guest);
    assert.equal(parsed.kind, "datagram");
    if (parsed.kind !== "datagram") throw new Error("test");
    assert.equal(parsed.destination, host);
    assert.equal(parsed.forwarded[0], tag + 2);
    assert.equal(hex(parsed.forwarded.subarray(1,33)), guest);
    assert.deepEqual(parsed.forwarded.subarray(33), frame.subarray(33));
  }
  const shortFinal = Uint8Array.of(5, ...peerBytes(host), 0, 4, 176, 9, 8, 7);
  const parsed = parseRelayFrame(shortFinal, guest);
  assert.equal(parsed.kind, "datagram");
  if (parsed.kind !== "datagram") throw new Error("test");
  assert.deepEqual(parsed.forwarded.subarray(33), shortFinal.subarray(33));
  assert.deepEqual(parseRelayFrame(Uint8Array.of(9, 1,2,3,4,5,6,7,8), guest), {kind:"ping", payload:Uint8Array.of(10,1,2,3,4,5,6,7,8)});
});
test("malformed, oversized, unknown and zero batch frames never forward", () => {
  const sender = "11".repeat(32);
  for (const input of [new Uint8Array(), new Uint8Array(MAX_RELAY_FRAME+1), Uint8Array.of(2), Uint8Array.of(9),
    Uint8Array.of(4,...peerBytes(sender),4,1), Uint8Array.of(5,...peerBytes(sender),0,0,0,1)]) {
    assert.throws(()=>parseRelayFrame(input,sender), /relay_invalid_frame/);
  }
});
test("frame budget fails closed on deadline, clock rollback, packet flood and exhaustion", () => {
  const start = newRelayBudget(1000);
  assert.throws(()=>debitRelayBudget(start, 1, 999, 2000));
  assert.throws(()=>debitRelayBudget(start, 1, 2000, 2000));
  assert.throws(()=>debitRelayBudget({...start, spent:268_435_456}, 1, 1001, 2000));
  assert.throws(()=>debitRelayBudget({...start, frames:1024}, 1, 1001, 2000));
  const softDrop = debitRelayBudget({...start, windowBytes:1_048_576}, 1, 1001, 2000);
  assert.equal(softDrop.rateLimited, true);
  assert.equal(softDrop.windowBytes, 1_048_576);
  assert.equal(softDrop.frames, 1);
  assert.equal(softDrop.spent, 0);
  assert.equal(debitRelayBudget({...start, windowBytes:1_048_576}, 4, 2000, 3000).windowBytes,4);
  assert.deepEqual(start,newRelayBudget(1000));
});
