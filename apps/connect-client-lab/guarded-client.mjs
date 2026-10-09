import { createRequire } from 'node:module';
import { Socket } from 'node:net';
import { FrameGuard } from '../connect-runtime-tools/frame-guard.mjs';
import { PLAY_NAMES } from '../connect-runtime-tools/play-profile.mjs';

// This module is an offline, local development client only. No optional login,
// token, session, Realms, profile cache, DNS or custom endpoint can be supplied.
if (process.env.DEBUG || process.env.NODE_OPTIONS || globalThis.debugNMP) throw new Error('client_unsafe_debug_environment');
const require = createRequire(import.meta.url);
const protocol = require('minecraft-protocol');
const blockedPackets = new Set(['compress', 'set_compression', 'encryption_begin', 'transfer',
  'resource_pack_send', 'add_resource_pack', 'code_of_conduct', 'start_configuration']);

export class GuardedClient extends protocol.Client {
  setSerializer(state) {
    super.setSerializer(state);
    const parse = this.deserializer.parsePacketBuffer.bind(this.deserializer);
    this.deserializer.parsePacketBuffer = buffer => {
      try {
        const packet = parse(buffer);
        if (packet.metadata?.size !== buffer.length) throw new Error();
        return packet;
      } catch {
        // The pinned FullPacketParser otherwise silently drops partialReadError
        // and accepts trailing bytes when hideErrors is set. A fresh fixed error
        // takes its normal failure path before any decoded event is delivered.
        throw new Error('client_packet_invalid');
      }
    };
  }
  notifyFailure(code) {
    this.protocolRejected = true;
    this.socket?.destroy();
    if (this.dispatchingFailure) return false;
    this.dispatchingFailure = true;
    // Upstream parser/chat handlers can throw synchronously (for example an
    // invalid DER key in player_info). Never let packet content escape as an
    // uncaught stack or bypass the caller's lifecycle finally blocks.
    try { return super.emit('error', new Error(code)); }
    catch { return false; }
    finally { this.dispatchingFailure = false; }
  }
  emit(event, ...args) {
    const packet = event === 'packet' ? args[1]?.name : event;
    if (blockedPackets.has(packet)) { this.rejectProtocol(); return false; }
    if (this.protocolRejected && !['error', 'end'].includes(event)) return false;
    if (event === 'error') {
      // The pinned keepalive handler generates this exact local message. Never
      // expose upstream text, inspect payload fields or invoke message getters.
      let keepaliveTimeout = false;
      try { keepaliveTimeout = Object.getOwnPropertyDescriptor(args[0], 'message')?.value
        === 'client timed out after 30000 milliseconds'; } catch { /* Unknown error stays redacted. */ }
      return this.notifyFailure(keepaliveTimeout ? 'client_keepalive_timeout' : 'client_protocol_failed');
    }
    try { return super.emit(event, ...args); }
    catch { return this.notifyFailure('client_protocol_handler_failed'); }
  }
  rejectProtocol() {
    if (this.protocolRejected) return;
    this.notifyFailure('client_protocol_refused');
  }
  setCompressionThreshold() { this.rejectProtocol(); }
  setEncryption() { this.rejectProtocol(); }
  setSocket(socket) {
    const guard = new FrameGuard();
    this.frameGuard = guard;
    guard.on('error', () => this.emit('error', new Error('client_frame_failed')));
    super.setSocket(socket);
    // Synchronous rewiring before this fresh Socket connects or receives data.
    // Outgoing frames still use the normal library serializer and socket.
    socket.unpipe(this.splitter);
    socket.pipe(guard).pipe(this.splitter);
    socket.once('close', () => guard.destroy());
  }
}

export function createOfflineClient(port, name) {
  if (!Number.isSafeInteger(port) || port < 1 || port > 65535 || !PLAY_NAMES.includes(name)) {
    throw new Error('client_invalid_target');
  }
  return protocol.createClient({ Client: GuardedClient, host: '127.0.0.1', port,
    username: name, version: '1.21.1', auth: 'offline', profilesFolder: false,
    // Match the pinned upstream default: the server's >=15s send cadence needs
    // tick/transport margin. The independent 60s caller deadline is unchanged.
    hideErrors: true, disableChatSigning: true, keepAlive: true, checkTimeoutInterval: 30000,
    clientSettings: { viewDistance: 2, chatFlags: 2, chatColors: false, skinParts: 0,
      enableTextFiltering: true, enableServerListing: false },
    connect: client => { const socket = new Socket(); client.setSocket(socket); socket.connect(port, '127.0.0.1'); },
  });
}

export async function closeOfflineClient(client) {
  const socket = client.socket;
  let forced = false;
  if (socket && !socket.closed) {
    await new Promise(resolve => {
      const timer = setTimeout(() => { forced = true; socket.destroy(); }, 1000);
      socket.once('close', () => { clearTimeout(timer); resolve(); });
      // Flush the complete serializer -> framer -> socket chain and mark the
      // serializer unwritable before late keepalives can enqueue more packets.
      // The independent one-second deadline still records forced cleanup.
      client.end('lab_complete');
    });
  }
  clearTimeout(client.closeTimer);
  for (const stream of [client.frameGuard, client.serializer, client.deserializer, client.splitter, client.framer]) stream?.destroy();
  client._mcBundle = [];
  return { closed: !socket || socket.closed, forced };
}
