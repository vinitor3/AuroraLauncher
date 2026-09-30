import { createOfflineClient, closeOfflineClient } from './guarded-client.mjs';
import { PlayEvidence } from './play-evidence.mjs';
import { PLAY_NAMES } from '../connect-runtime-tools/play-profile.mjs';
import { probePlasmoControl } from './plasmo-control.mjs';

const code = error => /^(client|plasmo_control)_[a-z_]+$/.test(error?.message ?? '') ? error.message : 'client_plasmo_failed';

// One synthetic game client, one private Plasmo control socket, no audio. Both
// exist only in this standalone client process. Secrets never reach its caller.
export async function observePlasmoPlayer(bridgePort, voicePort, { signal, timeoutMs = 60000 } = {}) {
  if (!Number.isSafeInteger(bridgePort) || bridgePort < 1 || bridgePort > 65535 || voicePort !== 35554
    || (signal !== undefined && !(signal instanceof AbortSignal))
    || !Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 60000) throw new Error('client_plasmo_arguments');
  signal?.throwIfAborted();
  const controller = new AbortController(); const evidence = new PlayEvidence(0);
  let client; let closing = false; let failure; let resolveReady; let rejectFailed; let probeWork; let result;
  const ready = new Promise(resolve => { resolveReady = resolve; });
  const failed = new Promise((_, reject) => { rejectFailed = reject; }); failed.catch(() => {});
  const fail = reason => {
    if (closing || failure) return;
    failure = new Error(reason); controller.abort(); rejectFailed(failure);
  };
  const abort = () => fail('client_plasmo_cancelled');
  signal?.addEventListener('abort', abort, { once: true });
  const deadline = setTimeout(() => fail('client_plasmo_deadline'), timeoutMs);
  try {
    client = createOfflineClient(bridgePort, PLAY_NAMES[0]);
    client.on('error', error => fail(code(error))); client.on('end', () => fail('client_plasmo_early_close'));
    client.on('packet', (packet, metadata) => {
      if (closing || failure) return;
      try {
        evidence.observe(metadata.name, packet, (name, data) => client.write(name, data));
        const state = evidence.summary();
        if (state.loginIdentity && state.configurationFinished && state.playLogin && state.positions > 0
          && state.chunks > 0 && state.chunkBatches > 0) resolveReady();
      } catch (error) { fail(code(error)); }
    });
    if (signal?.aborted) abort();
    // Plasmo requests PlayerInfo at join. Register before PLAY/world packets so
    // the real request cannot be lost while waiting for chunks.
    probeWork = probePlasmoControl(client, { approvedPort: voicePort, signal: controller.signal, timeoutMs: 10000 });
    probeWork.catch(error => fail(code(error)));
    await Promise.race([ready, failed]);
    const voice = await Promise.race([probeWork, failed]);
    if (failure) throw failure;
    const state = evidence.summary();
    result = { voice, game: Object.fromEntries(['loginIdentity', 'configurationFinished', 'playLogin', 'positions', 'chunks', 'chunkBatches', 'packets']
      .map(key => [key, state[key]])), minecraft: '1.21.1', protocol: 767,
      accountAuthentication: 'offline_synthetic_only', graphicalClientHomologated: false, runtimeHomologated: false };
  } catch (error) { failure ??= new Error(code(error)); }
  finally {
    closing = true; controller.abort(); clearTimeout(deadline); signal?.removeEventListener('abort', abort);
    if (probeWork) await probeWork.catch(() => {});
    if (client) {
      const cleanup = await closeOfflineClient(client).catch(() => ({ closed: false, forced: true }));
      if (!cleanup.closed || cleanup.forced) failure ??= new Error('client_plasmo_cleanup_failed');
      if (failure) failure.cleanup = cleanup;
      else result.cleanup = cleanup;
    }
  }
  if (failure) throw failure;
  return result;
}
