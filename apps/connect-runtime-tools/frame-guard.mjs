import { Transform } from 'node:stream';

// Minecraft's outer packet length is at most 21 bits. This is a wire framing
// guard, not a packet/schema validator or a decompression guard.
export const MAX_FRAME_BYTES = 0x1fffff;
export const MAX_TOTAL_BYTES = 32 * 1024 * 1024;

export class FrameGuardError extends Error {
  constructor(code) { super(code); this.name = 'FrameGuardError'; this.code = code; }
}
const failure = code => new FrameGuardError(code);

/**
 * Place before minecraft-protocol's Splitter. Complete validated frames,
 * including their original canonical length prefix, are forwarded unchanged.
 * No partial frame is visible downstream. An individual frame must be nonempty.
 *
 * A caller supplies its deadline through signal and must honor write(false), as
 * with every Node writable stream. socket.pipe(guard) does so automatically.
 * Allocation is one exact frame plus a three-byte header and the current input
 * chunk; no growing concatenation is used. Readable output stops on push(false).
 * The guard retains at most the current input chunk while applying backpressure;
 * it cannot undo oversized buffers already allocated/queued by a misbehaving
 * direct writer. Total accounting covers input wire bytes, including prefixes.
 */
export class FrameGuard extends Transform {
  #maxFrame;
  #maxTotal;
  #signal;
  #abort;
  #header = Buffer.alloc(3);
  #headerLength = 0;
  #payloadLength = 0;
  #frame;
  #filled = 0;
  #pending;
  #blocked = false;
  #draining = false;
  #receivedBytes = 0;
  #forwardedBytes = 0;
  #forwardedFrames = 0;
  #peakFrameAllocation = 0;

  constructor({ maxFrameBytes = MAX_FRAME_BYTES, maxTotalBytes = MAX_TOTAL_BYTES, signal } = {}) {
    super({ readableHighWaterMark: 16 * 1024, writableHighWaterMark: 16 * 1024 });
    if (!Number.isSafeInteger(maxFrameBytes) || maxFrameBytes < 1 || maxFrameBytes > MAX_FRAME_BYTES
      || !Number.isSafeInteger(maxTotalBytes) || maxTotalBytes < 1 || maxTotalBytes > MAX_TOTAL_BYTES
      || (signal !== undefined && !(signal instanceof AbortSignal))) throw failure('frame_guard_invalid_options');
    if (signal?.aborted) throw failure('frame_guard_cancelled');
    this.#maxFrame = maxFrameBytes; this.#maxTotal = maxTotalBytes; this.#signal = signal;
    this.#abort = () => this.destroy(failure('frame_guard_cancelled'));
    signal?.addEventListener('abort', this.#abort, { once: true });
  }

  get stats() {
    return Object.freeze({ receivedBytes: this.#receivedBytes, forwardedBytes: this.#forwardedBytes,
      forwardedFrames: this.#forwardedFrames, frameAllocation: this.#frame?.length ?? 0,
      peakFrameAllocation: this.#peakFrameAllocation, bufferedFrameBytes: this.#filled,
      pendingInputBytes: this.#pending ? this.#pending.chunk.length - this.#pending.offset : 0 });
  }

  _transform(chunk, _encoding, callback) {
    if (chunk.length > this.#maxTotal - this.#receivedBytes) {
      this.#receivedBytes += chunk.length;
      callback(failure('frame_guard_total_limit')); return;
    }
    this.#receivedBytes += chunk.length;
    this.#pending = { chunk, offset: 0, callback };
    this.#drain();
  }

  _read(size) {
    if (this.#pending) {
      this.#blocked = false;
      this.#drain();
    } else {
      super._read(size);
    }
  }

  #drain() {
    if (this.#draining || this.#blocked || this.destroyed || !this.#pending) return;
    this.#draining = true;
    let callback;
    let error;
    try {
      const input = this.#pending;
      while (input.offset < input.chunk.length && !this.#blocked && !this.destroyed) {
        if (!this.#frame) {
          const byte = input.chunk[input.offset++];
          this.#header[this.#headerLength] = byte;
          this.#payloadLength += (byte & 0x7f) * (2 ** (7 * this.#headerLength));
          this.#headerLength++;
          if (byte & 0x80) {
            if (this.#headerLength === 3) throw failure('frame_guard_varint');
            continue;
          }
          if ((this.#headerLength > 1 && byte === 0) || this.#payloadLength === 0) throw failure('frame_guard_varint');
          if (this.#payloadLength > this.#maxFrame) throw failure('frame_guard_frame_limit');
          const consumed = this.#receivedBytes - (input.chunk.length - input.offset);
          if (this.#payloadLength > this.#maxTotal - consumed) throw failure('frame_guard_total_limit');
          this.#frame = Buffer.allocUnsafe(this.#headerLength + this.#payloadLength);
          this.#header.copy(this.#frame, 0, 0, this.#headerLength);
          this.#filled = this.#headerLength;
          this.#peakFrameAllocation = Math.max(this.#peakFrameAllocation, this.#frame.length);
        }
        const take = Math.min(this.#frame.length - this.#filled, input.chunk.length - input.offset);
        input.chunk.copy(this.#frame, this.#filled, input.offset, input.offset + take);
        input.offset += take; this.#filled += take;
        if (this.#filled === this.#frame.length) {
          const complete = this.#frame;
          this.#frame = undefined; this.#filled = 0; this.#headerLength = 0; this.#payloadLength = 0;
          this.#forwardedFrames++; this.#forwardedBytes += complete.length;
          this.#blocked = !this.push(complete);
        }
      }
      // A pending callback is retained even if push(false) occurred at the very
      // end of this chunk. More upstream bytes are admitted only after _read.
      if (!this.#blocked && input.offset === input.chunk.length) {
        callback = input.callback; this.#pending = undefined;
      }
    } catch (caught) {
      error = caught instanceof FrameGuardError ? caught : failure('frame_guard_stream_error');
      callback = this.#pending?.callback; this.#pending = undefined;
    } finally { this.#draining = false; }
    // A synchronous writable callback may immediately invoke _transform again;
    // do not keep the reentrancy guard set while handing control back to Node.
    callback?.(error);
  }

  _flush(callback) {
    if (this.#headerLength || this.#frame) callback(failure('frame_guard_truncated'));
    else callback();
  }

  _destroy(error, callback) {
    this.#signal?.removeEventListener('abort', this.#abort);
    const safe = error == null ? null : error instanceof FrameGuardError ? error : failure('frame_guard_stream_error');
    const pending = this.#pending;
    this.#pending = undefined; this.#frame = undefined; this.#headerLength = 0;
    this.#payloadLength = 0; this.#filled = 0; this.#blocked = false;
    pending?.callback(safe ?? failure('frame_guard_closed'));
    callback(safe);
  }
}
