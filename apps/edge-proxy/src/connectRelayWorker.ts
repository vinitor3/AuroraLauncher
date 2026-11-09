import { DurableObject } from "cloudflare:workers";
import type { SocialSessionAuthority } from "./connectSocialWorker.ts";
import {
  RELAY_PROTOCOL, MAX_RELAY_FRAME, createRelayChallenge, verifyRelayAuthentication,
  parseRelayFrame, newRelayBudget, debitRelayBudget, relayWindow, peerBytes, type RelayBudget,
} from "./connectRelayProtocol.ts";

type Ticket = { body: Record<string, unknown>; signature: string };
type Authorization = { body: Record<string, unknown>; allowedPeers: string[]; freshUntil: number };
interface RelayEnv {
  SOCIAL_SESSIONS: DurableObjectNamespace<SocialSessionAuthority>;
  RELAY_SESSIONS: DurableObjectNamespace<ConnectRelaySession>;
}
type Attachment = {
  ticket: Ticket; peer: string; host: string; phase: "challenge" | "authenticating" | "ready";
  challenge: number[]; admissionDeadline: number; deadline: number; freshUntil: number;
  peers: string[]; budget: RelayBudget; reserved: number; reserving: boolean;
  reservedWindow: number | null; blockedWindow: number | null; rateNotified: boolean;
};
type Original = { issuer: string; host: string; deadline: number };
type Quota = { bytes: number; authentications: number };
const CHUNK_BYTES = 1_048_576;
const PEER_QUOTA = { bytes: 256 * CHUNK_BYTES, authentications: 64 };
const SESSION_QUOTA = { bytes: 1024 * CHUNK_BYTES, authentications: 256 };
const MAX_SESSION_MS = 14_400_000;
const MAX_FRESH_MS = 3000;
const UUID = /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/;
const HEX_KEY = /^[0-9a-f]{64}$/;
const emptyQuota = (): Quota => ({ bytes: 0, authentications: 0 });
const bad = (): never => { throw new Error("relay_unauthorized"); };

function ticketFromHeader(request: Request): Ticket {
  const header = request.headers.get("authorization");
  if (!header?.startsWith("Bearer ") || header.length > 4103 || !/^[\x20-\x7e]+$/.test(header)) bad();
  const ticket = JSON.parse(header!.slice(7)) as Ticket;
  if (!ticket || typeof ticket !== "object" || Object.keys(ticket).sort().join() !== "body,signature"
      || typeof ticket.signature !== "string" || !/^[0-9a-f]{128}$/.test(ticket.signature)) bad();
  const body = ticket.body;
  const now = Date.now();
  if (!body || body.schemaVersion !== 1 || typeof body.sessionId !== "string" || !UUID.test(body.sessionId)
      || typeof body.issuerInstance !== "string" || !HEX_KEY.test(body.issuerInstance)
      || typeof body.peerId !== "string" || !HEX_KEY.test(body.peerId)
      || typeof body.hostPeerId !== "string" || !HEX_KEY.test(body.hostPeerId)
      || typeof body.issuedAtUnixMs !== "number" || !Number.isSafeInteger(body.issuedAtUnixMs)
      || typeof body.expiresAtUnixMs !== "number" || !Number.isSafeInteger(body.expiresAtUnixMs)
      || body.issuedAtUnixMs > now || body.expiresAtUnixMs <= now
      || body.expiresAtUnixMs - body.issuedAtUnixMs > MAX_SESSION_MS) bad();
  return ticket;
}

function attachment(socket: WebSocket): Attachment | null {
  return socket.deserializeAttachment() as Attachment | null;
}

function close(socket: WebSocket, code = 1008) {
  try { socket.close(code, "relay closed"); } catch { /* already closed */ }
}

function live(socket: WebSocket, state: Attachment | null, now: number): state is Attachment {
  return state?.phase === "ready" && socket.readyState === WebSocket.OPEN
    && now >= state.budget.startMs && now < state.deadline && now < state.freshUntil;
}

function authorizationWindow(authorized: Authorization, ticket: Ticket, now: number) {
  const { body, allowedPeers, freshUntil } = authorized;
  for (const field of ["sessionId", "issuerInstance", "peerId", "hostPeerId", "expiresAtUnixMs"]) {
    if (body[field] !== ticket.body[field]) bad();
  }
  if (!Number.isSafeInteger(freshUntil) || freshUntil <= now
      || freshUntil > now + MAX_FRESH_MS || freshUntil > Number(body.expiresAtUnixMs)
      || !Array.isArray(allowedPeers) || allowedPeers.length > 8
      || allowedPeers.some(peer => typeof peer !== "string" || !HEX_KEY.test(peer))
      || new Set(allowedPeers).size !== allowedPeers.length) bad();
  return { peers: allowedPeers, freshUntil };
}

/** Route only authenticated Iroh keys within their original Aurora session. */
export async function routeRelayRequest(request: Request, env: RelayEnv): Promise<Response> {
  try {
    const url = new URL(request.url);
    if (request.method !== "GET" || url.search || request.headers.has("origin")) bad();
    if (url.pathname === "/ping") return new Response(null, { status: 204, headers: { "Cache-Control": "no-store" } });
    if (url.pathname !== "/relay" || request.headers.get("upgrade")?.toLowerCase() !== "websocket"
      || !request.headers.get("sec-websocket-protocol")?.split(",").some(p => p.trim() === RELAY_PROTOCOL)) bad();
    const ticket = ticketFromHeader(request);
    return await env.RELAY_SESSIONS.getByName(String(ticket.body.sessionId)).fetch(request);
  } catch {
    return Response.json({ code: "relay_unauthorized" }, { status: 403, headers: { "Cache-Control": "no-store" } });
  }
}

export class ConnectRelaySession extends DurableObject<RelayEnv> {
  private async authorize(ticket: Ticket) {
    const authorization = await this.env.SOCIAL_SESSIONS.getByName(String(ticket.body.sessionId)).relayAuthorize(ticket);
    return authorizationWindow(authorization, ticket, Date.now());
  }

  async fetch(request: Request): Promise<Response> {
    const ticket = ticketFromHeader(request);
    if (this.ctx.id.name !== ticket.body.sessionId) bad();
    const authorization = await this.authorize(ticket);
    const peer = String(ticket.body.peerId);
    const host = String(ticket.body.hostPeerId);
    const original: Original = {
      issuer: String(ticket.body.issuerInstance), host, deadline: Number(ticket.body.expiresAtUnixMs),
    };
    await this.ctx.storage.transaction(async txn => {
      const retained = await txn.get<Original>("original");
      if (retained && (retained.issuer !== original.issuer || retained.host !== original.host
          || retained.deadline !== original.deadline)) bad();
      if (!retained) await txn.put("original", original);
    });
    await this.schedule(original.deadline);
    const now = Date.now();
    if (now >= original.deadline || now >= authorization.freshUntil) bad();
    const sockets = this.ctx.getWebSockets();
    if (sockets.length >= 18 || sockets.filter(socket => attachment(socket)?.peer === peer).length >= 2) bad();
    const challenge = createRelayChallenge();
    const pair = new WebSocketPair();
    this.ctx.acceptWebSocket(pair[1]);
    // Retain the signed routing ticket with the socket for authority revalidation.
    pair[1].serializeAttachment({
      ticket, peer, host, phase: "challenge", challenge: Array.from(challenge),
      admissionDeadline: now + 10_000, deadline: original.deadline, ...authorization,
      budget: newRelayBudget(now), reserved: 0, reserving: false,
      reservedWindow: null, blockedWindow: null, rateNotified: false,
    } satisfies Attachment);
    pair[1].send(challenge);
    await this.schedule(now + 1000);
    return new Response(null, { status: 101, webSocket: pair[0], headers: { "Sec-WebSocket-Protocol": RELAY_PROTOCOL } });
  }

  private async debit(peer: string, deadline: number, charge: Quota, window?: number): Promise<boolean> {
    return await this.ctx.storage.transaction(async txn => {
      const original = await txn.get<Original>("original");
      if (!original || deadline !== original.deadline || Date.now() >= deadline) bad();
      if (window !== undefined) {
        const previous = await txn.get<number>(`rate:${peer}`);
        if (previous !== undefined && previous > window) bad();
        if (previous === window) return false;
        // Reserve the entire window to one socket. On reconnect or eviction,
        // unused byte/frame capacity cannot be spent a second time.
        await txn.put(`rate:${peer}`, window);
      }
      if (charge.bytes === 0 && charge.authentications === 0) return true;
      const amounts: [string, Quota][] = [["quota:session", SESSION_QUOTA], [`quota:${peer}`, PEER_QUOTA]];
      for (const [key, limit] of amounts) {
        const current = await txn.get<Quota>(key) ?? emptyQuota();
        const next = { bytes: current.bytes + charge.bytes, authentications: current.authentications + charge.authentications };
        if (next.bytes > limit.bytes || next.authentications > limit.authentications) bad();
        await txn.put(key, next);
      }
      return true;
    });
  }

  private rateLimited(socket: WebSocket, state: Attachment) {
    const notify = !state.rateNotified;
    state.rateNotified = true;
    socket.serializeAttachment(state);
    // Iroh relay protocol v2 Status::RateLimited, sent once per connection.
    if (notify) socket.send(Uint8Array.of(13, 2));
  }

  private async authenticate(socket: WebSocket, state: Attachment, frame: Uint8Array) {
    if (Date.now() >= state.admissionDeadline) bad();
    state.phase = "authenticating";
    socket.serializeAttachment(state);
    if (!await verifyRelayAuthentication(Uint8Array.from(state.challenge), frame, state.peer)) bad();
    const authorization = await this.authorize(state.ticket);
    await this.debit(state.peer, state.deadline, { bytes: 0, authentications: 1 });
    const now = Date.now();
    if (socket.readyState !== WebSocket.OPEN || now >= state.admissionDeadline || now >= authorization.freshUntil) bad();
    const ready = { ...state, ...authorization, challenge: [], phase: "ready" } satisfies Attachment;
    socket.serializeAttachment(ready);
    // A proven reconnect replaces the previous ready route. Pending challenges
    // cannot displace an existing connection.
    for (const old of this.ctx.getWebSockets()) {
      const previous = attachment(old);
      if (old !== socket && previous?.peer === state.peer && previous.phase === "ready") close(old);
    }
    socket.send(Uint8Array.of(2));
    await this.schedule(Math.max(now + 100, authorization.freshUntil - 500));
  }

  async webSocketMessage(socket: WebSocket, data: ArrayBuffer | string) {
    try {
      if (typeof data === "string" || data.byteLength > MAX_RELAY_FRAME) bad();
      let state = attachment(socket);
      const now = Date.now(), frame = new Uint8Array(data as ArrayBuffer);
      if (!state || now >= state.deadline || now < state.budget.startMs) return close(socket);
      if (state.phase === "challenge") return await this.authenticate(socket, state, frame);
      if (state.phase !== "ready") return close(socket);
      // The alarm refreshes authority. QUIC can retry packets while authority is stale.
      if (!live(socket, state, now)) return;
      state.budget = debitRelayBudget(state.budget, frame.length, now, state.deadline);
      const parsed = parseRelayFrame(frame, state.peer);
      if (parsed.kind === "datagram" && (!state.peers.includes(parsed.destination)
        || (state.peer !== state.host && parsed.destination !== state.host))) bad();
      const window = state.budget.windowMs;
      if (state.budget.rateLimited || state.blockedWindow === window) return this.rateLimited(socket, state);
      const needsWindow = state.reservedWindow !== window;
      const needsBytes = state.reserved < frame.length;
      if (needsWindow || needsBytes) {
        const shouldReserve = !state.reserving;
        state.reserving = true;
        socket.serializeAttachment(state);
        if (!shouldReserve) return;
        // Durability precedes forwarding. While a reservation is pending,
        // later packets update the rate budget but cannot spend this credit.
        const bytes = needsBytes ? CHUNK_BYTES : 0;
        const booked = await this.debit(state.peer, state.deadline, { bytes, authentications: 0 },
          needsWindow ? window : undefined);
        const latest = attachment(socket);
        if (!latest || socket.readyState !== WebSocket.OPEN) return;
        state = latest;
        state.reserving = false;
        if (!booked) {
          state.blockedWindow = window;
          return this.rateLimited(socket, state);
        }
        state.reservedWindow = window;
        state.reserved += bytes;
      }
      const checkedAt = Date.now();
      if (relayWindow(checkedAt) !== window) {
        socket.serializeAttachment(state);
        return;
      }
      state.reserved -= frame.length;
      socket.serializeAttachment(state);
      if (!live(socket, state, checkedAt)) return;
      if (parsed.kind === "ping") { socket.send(parsed.payload); return; }
      if (parsed.kind === "pong" || !state.peers.includes(parsed.destination)) return;
      const target = this.ctx.getWebSockets().find(candidate => {
        const destination = attachment(candidate);
        return live(candidate, destination, checkedAt) && destination.peer === parsed.destination
          && destination.host === state.host && destination.peers.includes(state.peer);
      });
      // Missing peers are dropped; Iroh's QUIC layer owns retransmission.
      target?.send(parsed.forwarded);
    } catch {
      close(socket);
    }
  }

  private async schedule(next: number) {
    const existing = await this.ctx.storage.getAlarm();
    if (existing === null || next < existing) await this.ctx.storage.setAlarm(next);
  }

  async alarm() {
    const now = Date.now();
    await Promise.all(this.ctx.getWebSockets().map(async socket => {
      const state = attachment(socket);
      try {
        if (!state || now >= state.deadline || now < state.budget.startMs
          || (state.phase !== "ready" && now >= state.admissionDeadline)) return close(socket);
        if (state.phase !== "ready") return;
        const current = await this.authorize(state.ticket);
        // RPC yields to packet processing. Only refresh the newest attachment.
        const latest = attachment(socket);
        if (!latest || socket.readyState !== WebSocket.OPEN || Date.now() >= latest.deadline) return close(socket);
        socket.serializeAttachment({ ...latest, ...current });
      } catch {
        close(socket);
      }
    }));
    const checkedAt = Date.now();
    const active = this.ctx.getWebSockets().filter(socket => socket.readyState === WebSocket.OPEN);
    if (active.length) {
      const due = active.map(socket => {
        const state = attachment(socket);
        if (!state) return checkedAt;
        return state.phase === "ready" ? Math.min(state.freshUntil - 500, state.deadline) : state.admissionDeadline;
      });
      await this.schedule(Math.max(checkedAt + 100, Math.min(checkedAt + 1000, ...due)));
    } else {
      const original = await this.ctx.storage.get<Original>("original");
      if (original && checkedAt < original.deadline) await this.schedule(original.deadline);
      else await this.ctx.storage.deleteAll();
    }
  }

  webSocketClose(socket: WebSocket, code: number) {
    const state = attachment(socket);
    const now = Date.now();
    const others = this.ctx.getWebSockets().filter(candidate => candidate !== socket);
    const replaced = others.some(candidate => {
      const peer = attachment(candidate);
      return live(candidate, peer, now) && peer.peer === state?.peer;
    });
    if (state?.phase === "ready" && !replaced) {
      for (const target of others) {
        const peer = attachment(target);
        if (live(target, peer, now) && peer.peers.includes(state.peer)
            && (state.peer === state.host || peer.peer === state.host)) {
          try { target.send(Uint8Array.of(8, ...peerBytes(state.peer))); } catch { close(target); }
        }
      }
    }
    close(socket, code === 1000 ? 1000 : 1008);
  }

  webSocketError(socket: WebSocket) { close(socket); }
}
