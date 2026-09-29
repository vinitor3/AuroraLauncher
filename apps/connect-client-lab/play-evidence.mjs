import { PLAY_NAMES, offlineUuid } from '../connect-runtime-tools/play-profile.mjs';

// Evidence accumulator for protocol 767, not a GUI/player implementation. It
// keeps the last synthetic position only to acknowledge relative teleports and
// never returns coordinates, names, UUIDs, chunks, chat or server text.
export class PlayEvidence {
  #name; #self; #peer;
  #position = { x: 0, y: 0, z: 0, yaw: 0, pitch: 0 };
  #seen = { loginIdentity: false, configurationFinished: false, playLogin: false,
    positions: 0, chunks: 0, chunkBatches: 0, peerInfo: false, peerSpawn: false, packets: 0 };
  constructor(index) {
    if (index !== 0 && index !== 1) throw new Error('client_evidence_identity');
    this.#name = PLAY_NAMES[index]; this.#self = offlineUuid(this.#name); this.#peer = offlineUuid(PLAY_NAMES[1 - index]);
  }
  get ready() {
    const s = this.#seen;
    return s.loginIdentity && s.configurationFinished && s.playLogin && s.positions > 0 && s.chunks > 0
      && s.chunkBatches > 0 && s.peerInfo && s.peerSpawn;
  }
  summary() { return { ...this.#seen, playAndWorldPacketsObserved: this.ready }; }
  observe(name, data, write) {
    if (++this.#seen.packets > 65536) throw new Error('client_packet_count_limit');
    if (name === 'success') {
      if (data.uuid !== this.#self || data.username !== this.#name) throw new Error('client_login_identity_mismatch');
      this.#seen.loginIdentity = true;
    } else if (name === 'finish_configuration') {
      if (!this.#seen.loginIdentity) throw new Error('client_evidence_order');
      this.#seen.configurationFinished = true;
    } else if (name === 'login') {
      if (!this.#seen.configurationFinished || !Number.isInteger(data.entityId)) throw new Error('client_evidence_order');
      this.#seen.playLogin = true;
    } else if (name === 'position') {
      if (!this.#seen.playLogin || !Number.isInteger(data.teleportId) || data.teleportId < 0 || !data.flags) throw new Error('client_position_invalid');
      if (!this.#seen.positions && ['x', 'y', 'z', 'yaw', 'pitch'].some(field => data.flags[field])) throw new Error('client_position_base_unknown');
      const position = {};
      for (const field of ['x', 'y', 'z', 'yaw', 'pitch']) {
        if (!Number.isFinite(data[field]) || typeof data.flags[field] !== 'boolean') throw new Error('client_position_invalid');
        position[field] = data[field] + (data.flags[field] ? this.#position[field] : 0);
        if (!Number.isFinite(position[field]) || Math.abs(position[field]) > 60_000_000) throw new Error('client_position_invalid');
      }
      this.#position = position;
      write('teleport_confirm', { teleportId: data.teleportId });
      write('position_look', { ...position, onGround: false });
      this.#seen.positions++;
    } else if (name === 'map_chunk') {
      if (!this.#seen.playLogin || !Buffer.isBuffer(data.chunkData) || !data.chunkData.length) throw new Error('client_chunk_invalid');
      this.#seen.chunks++;
    } else if (name === 'chunk_batch_finished') {
      if (!this.#seen.playLogin || !Number.isInteger(data.batchSize) || data.batchSize < 0 || data.batchSize > 4096) throw new Error('client_chunk_invalid');
      write('chunk_batch_received', { chunksPerTick: 1 }); this.#seen.chunkBatches++;
    } else if (name === 'ping') {
      if (!Number.isInteger(data.id)) throw new Error('client_ping_invalid');
      write('pong', { id: data.id });
    } else if (name === 'player_info' && data.action?.add_player === true) {
      if (!Array.isArray(data.data) || data.data.length > 256) throw new Error('client_peer_info_invalid');
      this.#seen.peerInfo ||= data.data.some(row => row.uuid === this.#peer);
    } else if (name === 'spawn_entity' && data.objectUUID === this.#peer) {
      if (data.type !== 128 || !['x', 'y', 'z'].every(field => Number.isFinite(data[field]))) throw new Error('client_peer_spawn_invalid');
      this.#seen.peerSpawn = true;
    } else if (name === 'disconnect' || name === 'kick_disconnect') {
      throw new Error('client_server_disconnect');
    }
  }
}
