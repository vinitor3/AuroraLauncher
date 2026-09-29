import { createOfflineClient, closeOfflineClient } from './guarded-client.mjs';
import { PlayEvidence } from './play-evidence.mjs';
import { PLAY_NAMES } from '../connect-runtime-tools/play-profile.mjs';

export async function observeTwoPlayers(ports, { signal, onReady, timeoutMs = 60000 } = {}) {
  if (!Array.isArray(ports) || ports.length !== 2 || ports[0] === ports[1]
    || ports.some(port => !Number.isSafeInteger(port) || port < 1 || port > 65535)
    || (signal !== undefined && !(signal instanceof AbortSignal))
    || (onReady !== undefined && typeof onReady !== 'function')
    || !Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 60000) throw new Error('client_pair_arguments');
  signal?.throwIfAborted();
  const controller = new AbortController();
  const clients = []; const evidence = [new PlayEvidence(0), new PlayEvidence(1)];
  let closing = false; let firstFailure; let resolveReady; let rejectFailed;
  const ready = new Promise(resolve => { resolveReady = resolve; });
  const failed = new Promise((_, reject) => { rejectFailed = reject; });
  failed.catch(() => {});
  const fail = code => {
    if (closing || firstFailure) return;
    firstFailure = code; controller.abort(); rejectFailed(new Error(code));
  };
  const aborted = () => fail('client_pair_cancelled');
  signal?.addEventListener('abort', aborted, { once: true });
  const deadline = setTimeout(() => fail('client_pair_deadline'), timeoutMs);
  let result; let operationFailure;
  const observation = () => evidence.map((item, index) => ({ ...item.summary(),
    frames: clients[index]?.frameGuard?.stats.forwardedFrames ?? 0,
    receivedBytes: clients[index]?.frameGuard?.stats.receivedBytes ?? 0,
    bufferedFrameBytes: clients[index]?.frameGuard?.stats.bufferedFrameBytes ?? 0,
    pendingInputBytes: clients[index]?.frameGuard?.stats.pendingInputBytes ?? 0,
    bufferedReadableBytes: clients[index]?.frameGuard?.readableLength ?? 0,
    bufferedWritableBytes: clients[index]?.frameGuard?.writableLength ?? 0 }));
  try {
    for (let index = 0; index < 2; index++) {
      const client = createOfflineClient(ports[index], PLAY_NAMES[index]); clients.push(client);
      client.on('error', error => fail(/^client_[a-z_]+$/.test(error.message) ? error.message : 'client_pair_failed'));
      client.on('end', () => fail('client_pair_early_close'));
      client.on('packet', (packet, metadata) => {
        if (closing || firstFailure) return;
        try {
          evidence[index].observe(metadata.name, packet, (name, data) => client.write(name, data));
          if (evidence.every(item => item.ready)) resolveReady();
        } catch (error) { fail(/^client_[a-z_]+$/.test(error.message) ? error.message : 'client_pair_failed'); }
      });
    }
    if (signal?.aborted) aborted();
    await Promise.race([ready, failed]);
    if (onReady) await Promise.race([Promise.resolve().then(() => onReady({ signal: controller.signal })), failed]);
    if (firstFailure) throw new Error(firstFailure);
    result = { event: 'headless_world_packets_observed', clients: evidence.map((item, i) => ({
      ...item.summary(), frames: clients[i].frameGuard.stats.forwardedFrames,
      receivedBytes: clients[i].frameGuard.stats.receivedBytes,
    })), minecraft: '1.21.1', protocol: 767, accountAuthentication: 'offline_synthetic_only',
    graphicalClientHomologated: false, coreSkins: 'not_tested', voice: 'not_tested', runtimeHomologated: false };
  } catch (error) {
    operationFailure = new Error(/^client_[a-z_]+$/.test(error?.message ?? '') ? error.message : 'client_pair_failed');
    operationFailure.observation = observation();
  } finally {
    closing = true; controller.abort(); clearTimeout(deadline); signal?.removeEventListener('abort', aborted);
    const settled = await Promise.allSettled(clients.map(closeOfflineClient));
    const cleanup = settled.map(item => item.status === 'fulfilled' ? item.value : { closed: false, forced: true });
    if (cleanup.some(item => !item.closed || item.forced)) {
      operationFailure ??= Object.assign(new Error('client_pair_cleanup_forced'), { observation: observation() });
    }
    if (operationFailure) operationFailure.cleanup = cleanup;
    else if (result) result.cleanup = 'both_owned_sockets_closed';
  }
  if (operationFailure) throw operationFailure;
  return result;
}
