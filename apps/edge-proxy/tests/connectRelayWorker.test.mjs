import assert from "node:assert/strict";
import { once } from "node:events";
import test from "node:test";
import { setTimeout as sleep } from "node:timers/promises";
import { blake3 } from "@noble/hashes/blake3.js";
import { WebSocket } from "ws";
import { startRelayFixture } from "./connectRelayMiniflare.mjs";

const session = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const chunk = 1_048_576;
const keypair = async () => {
  const keys = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]);
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", keys.publicKey));
  return { keys, raw, id: Buffer.from(raw).toString("hex") };
};
const ticket = (peer, host, now) => ({
  body: {
    sessionId: session, schemaVersion: 1, issuerInstance: "f".repeat(64),
    hostPeerId: host.id, peerId: peer.id, accountId: "fixture", generation: 1,
    issuedAtUnixMs: now, expiresAtUnixMs: now + 20_000, directAddresses: [],
  },
  signature: "0".repeat(128),
});
const event = (target, name) => once(target, name, { signal: AbortSignal.timeout(4000) });
const nextWindow = () => sleep(1050 - Date.now() % 1000);

async function prove(address, identity, authority) {
  const url = new URL("/relay", address);
  url.protocol = "ws:";
  const socket = new WebSocket(url, "iroh-relay-v2", {
    headers: { authorization: "Bearer " + JSON.stringify(authority) },
  });
  try {
    const [challenge] = await event(socket, "message");
    assert.equal(challenge.length, 17);
    const message = blake3(challenge.subarray(1), {
      context: new TextEncoder().encode("iroh-relay handshake v1 challenge signature"),
    });
    const signature = new Uint8Array(await crypto.subtle.sign("Ed25519", identity.keys.privateKey, message));
    socket.send(Uint8Array.of(1, ...identity.raw, 64, ...signature));
    return socket;
  } catch (error) {
    socket.terminate();
    throw error;
  }
}

async function ready(address, identity, authority) {
  const socket = await prove(address, identity, authority);
  try {
    const [confirmation] = await event(socket, "message");
    assert.deepEqual([...confirmation], [2]);
    return socket;
  } catch (error) {
    socket.terminate();
    throw error;
  }
}

async function disconnect(socket) {
  const closed = event(socket, "close");
  socket.close();
  await closed;
}

test("Miniflare relay keeps durable quotas through disconnect and atomically refuses exhausted routes", { timeout: 15_000 }, async () => {
  const [host, guest] = await Promise.all([keypair(), keypair()]);
  const now = Date.now(), hostTicket = ticket(host, host, now), guestTicket = ticket(guest, host, now);
  const { runtime, address } = await startRelayFixture([
    { ticket: hostTicket, allowedPeers: [guest.id] },
    { ticket: guestTicket, allowedPeers: [host.id] },
  ]);
  const sockets = [];
  try {
    const relay = (await runtime.getDurableObjectNamespace("RELAY_SESSIONS")).getByName(session);
    const actor = (await runtime.getDurableObjectNamespace("SOCIAL_SESSIONS")).getByName(session);
    const stored = async key => JSON.parse(await relay.fixtureRead(key));
    const hostSocket = await ready(address, host, hostTicket);
    const guestSocket = await ready(address, guest, guestTicket);
    sockets.push(hostSocket, guestSocket);
    await nextWindow();
    guestSocket.send(Uint8Array.of(4, ...host.raw, 0, 42));
    const [forwarded] = await event(hostSocket, "message");
    assert.deepEqual([...forwarded], [6, ...guest.raw, 0, 42]);
    assert.deepEqual(await stored("quota:session"), { bytes: chunk, authentications: 2 });
    assert.deepEqual(await stored("quota:" + guest.id), { bytes: chunk, authentications: 1 });

    await actor.fixtureRevoke(guest.id);
    const revoked = event(guestSocket, "close");
    await relay.fixtureAlarm();
    assert.equal((await revoked)[0], 1008);
    await disconnect(hostSocket);
    await relay.fixtureAlarm();
    assert.deepEqual(await stored("quota:session"), { bytes: chunk, authentications: 2 });

    await relay.fixtureSeed("quota:session", { bytes: 1024 * chunk, authentications: 2 });
    const reconnect = await ready(address, host, hostTicket);
    sockets.push(reconnect);
    const exhausted = event(reconnect, "close");
    reconnect.send(Uint8Array.of(9, 1, 2, 3, 4, 5, 6, 7, 8));
    assert.equal((await exhausted)[0], 1008);
    assert.deepEqual(await stored("quota:session"), { bytes: 1024 * chunk, authentications: 3 });
    assert.deepEqual(await stored("quota:" + host.id), { bytes: 0, authentications: 2 });

    await relay.fixtureSeed("quota:" + host.id, { bytes: 0, authentications: 64 });
    const denied = await prove(address, host, hostTicket);
    sockets.push(denied);
    assert.equal((await event(denied, "close"))[0], 1008);
    assert.deepEqual(await stored("quota:session"), { bytes: 1024 * chunk, authentications: 3 });
  } finally {
    for (const socket of sockets) socket.terminate();
    await runtime.dispose();
  }
});


test("a reserved rate window survives reconnect, signals throttling and permits the next window", { timeout: 12_000 }, async () => {
  const host = await keypair();
  const authority = ticket(host, host, Date.now());
  const { runtime, address } = await startRelayFixture([{ ticket: authority, allowedPeers: [] }]);
  let socket;
  try {
    const relay = (await runtime.getDurableObjectNamespace("RELAY_SESSIONS")).getByName(session);
    socket = await ready(address, host, authority);
    await nextWindow();
    const ping = Uint8Array.of(9, 1, 2, 3, 4, 5, 6, 7, 8);
    socket.send(ping);
    assert.deepEqual([...(await event(socket, "message"))[0]], [10, ...ping.subarray(1)]);
    const booked = JSON.parse(await relay.fixtureRead("rate:" + host.id));
    assert.equal(booked, Math.floor(Date.now() / 1000) * 1000);
    await disconnect(socket);

    socket = await ready(address, host, authority);
    socket.send(ping);
    assert.deepEqual([...(await event(socket, "message"))[0]], [13, 2]);
    assert.equal(JSON.parse(await relay.fixtureRead("rate:" + host.id)), booked);
    await nextWindow();
    socket.send(ping);
    assert.deepEqual([...(await event(socket, "message"))[0]], [10, ...ping.subarray(1)]);
    assert.ok(JSON.parse(await relay.fixtureRead("rate:" + host.id)) > booked);
  } finally {
    socket?.terminate();
    await runtime.dispose();
  }
});
