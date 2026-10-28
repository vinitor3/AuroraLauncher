import assert from 'node:assert/strict';
import { test } from 'node:test';
import { isAbsolute } from 'node:path';
import { stat } from 'node:fs/promises';
import { startSocialBindingLab } from './social-binding-server.mjs';
import { account, activeTcp, echo, manager, prepareHost, prepareGuest, rebind, socialFactory, tcpRoundtrip, waitSnapshot } from './social-desktop-fixtures.mjs';

const enabled = process.env.AURORA_SOCIAL_DESKTOP_INTEGRATION === '1';
test('89 launcher ConnectManager joins social sessions through authenticated local broker', { skip: !enabled, timeout: 240_000 }, async t => {
  const binary = process.env.AURORA_SOCIAL_DESKTOP_TEST_BINARY;
  assert.ok(binary && isAbsolute(binary), 'absolute reviewed desktop test binary required');
  assert.ok((await stat(binary)).isFile());
  const factory = await socialFactory();
  async function scenario(visibility, ending) {
    let lab, host, guest, source, hostClient, guestClient, guestReservation, bridge, active;
    let labClosed = false, primary, cleanupFailure;
    const started = performance.now();
    try {
      lab = await startSocialBindingLab({ enabled: true, port: 42888, admission: true });
      host = await account(factory); guest = await account(factory); source = await echo();
      if (visibility === 'friends' && ending !== 'nonfriend') {
        await host.service.requestFriend(guest.uid); await guest.service.respondFriend(host.uid, 'accept');
      }
      await host.service.publishSession({ title: 'Launcher local session', modpackName: 'Social Connect 89', minecraftVersion: '1.21.1', loader: 'Fabric',
        imageUrl: null, players: 1, maxPlayers: 2, visibility, joinPolicy: 'open' });
      const sessionId = await host.sessionId();
      hostClient = await manager(binary, lab); guestClient = await manager(binary, lab);
      const hostReservation = await prepareHost(hostClient, host, sessionId, visibility);
      if (ending === 'blocked-before') await host.service.block(guest.uid);
      guestReservation = await prepareGuest(guestClient, guest, sessionId);
      if (ending === 'nonfriend' || ending === 'blocked-before') {
        const failed = await waitSnapshot(guestClient, value => value.phase === 'failed' && value.cleanupState === 'complete');
        assert.equal(failed.social.stage, 'failed'); assert.equal(failed.tcpAddress, null);
        assert.equal(source.connections, 0); assert.equal(source.received, 0);
        const stopped = await hostClient.ok('stop', { operationId: hostReservation.operationId });
        assert.equal(stopped.cleanupState, 'complete');
        t.diagnostic(JSON.stringify({ case: ending, denied: true, gameConnections: 0, tcpBytes: 0, elapsedMs: Math.round(performance.now() - started) }));
        return;
      }
      const connected = await waitSnapshot(guestClient, value => value.phase === 'guestConnected' && value.social?.stage === 'connected');
      bridge = connected.tcpAddress; assert.match(bridge, /^127\.0\.0\.1:\d+$/);
      await tcpRoundtrip(bridge);
      const hostState = await waitSnapshot(hostClient, value => value.social?.stage === 'connected');
      assert.equal(hostState.operationId, hostReservation.operationId);
      active = await activeTcp(bridge);
      const remainingSessionMs = Math.min(hostState.social.expiresAtMs, connected.social.expiresAtMs) - Date.now();
      assert.ok(remainingSessionMs > 10_000, 'withdrawal must precede ordinary session expiry');
      assert.equal(active.isOpen, true, 'active flow must be open before withdrawal trigger');
      const closeStarted = performance.now();
      // Observe closure independently, release our own half-open side promptly,
      // then await native cleanup/rebind. Do not keep a fixture peer alive while
      // waiting for the native owner to prove it released the same port.
      const withdrawn = active.withdrawn();
      withdrawn.catch(() => {});
      if (ending === 'block') await host.service.block(guest.uid);
      else if (ending === 'backend-loss') { await lab.close(); labClosed = true; }
      else if (ending === 'account-change') {
        const closed = await guestClient.ok('accountTransition');
        assert.equal(closed.cleanupState, 'complete'); assert.ok(['closed', 'failed'].includes(closed.phase));
        const stale = await guestClient.call('reserve', { request: { accountEpoch: guestClient.ready.capabilities.accountEpoch, accountId: guest.uid,
          sessionId, role: 'guest', gameTarget: null, visibility: null } });
        assert.equal(stale.ok, false, 'old account epoch must not reserve after logout');
        const tokenLate = await guestClient.call('authenticate', { ...guestReservation, token: await guest.token() });
        assert.equal(tokenLate.ok, false, 'late token must not revive closed operation');
      } else throw new Error('UNKNOWN_ENDING');
      await withdrawn;
      const [hostClosed, guestClosed] = await Promise.all([
        waitSnapshot(hostClient, value => ['closed', 'failed'].includes(value.phase) && value.cleanupState === 'complete'),
        waitSnapshot(guestClient, value => ['closed', 'failed'].includes(value.phase) && value.cleanupState === 'complete'),
      ]);
      const withdrawalMs = Math.round(performance.now() - closeStarted);
      assert.equal(hostClosed.transportState, 'closed'); assert.equal(guestClosed.transportState, 'closed');
      assert.equal(guestClosed.tcpAddress, null); await rebind(bridge);
      t.diagnostic(JSON.stringify({ case: `${visibility}-${ending}`, tcpBytesEachDirection: 1_048_576, automaticCleanup: true,
        tcpRebind: true, activeTcpWithdrawn: true, activeTcpBytesReceived: active.received, remainingSessionMs, withdrawalMs, elapsedMs: Math.round(performance.now() - started), minecraft: 'not_tested', voice: 'not_tested' }));
    } catch (error) { primary = error; }
    finally {
      // Preserve the original error while still reporting any teardown failure.
      active?.close();
      const cleanups = [guestClient && (() => guestClient.close()), hostClient && (() => hostClient.close()),
        source && (() => source.close()), guest && (() => guest.close()), host && (() => host.close()),
        lab && !labClosed && (() => lab.close())].filter(Boolean);
      for (const cleanup of cleanups) { try { await cleanup(); } catch (error) { cleanupFailure ??= error; } }
      if (source) { try { await rebind('127.0.0.1:43001'); } catch (error) { cleanupFailure ??= error; } }
      if (primary && cleanupFailure) throw new AggregateError([primary, cleanupFailure], 'SCENARIO_AND_CLEANUP_FAILED');
      if (cleanupFailure) throw cleanupFailure;
    }
    if (primary) throw primary;
  }
  await t.test('public Join transfers bytes and blocking closes native workers', () => scenario('public', 'block'));
  await t.test('accepted friend Join transfers bytes and account change fences late calls', () => scenario('friends', 'account-change'));
  await t.test('backend loss closes both native workers without a stop command', () => scenario('public', 'backend-loss'));
  await t.test('nonfriend cannot open a friends game source', () => scenario('friends', 'nonfriend'));
  await t.test('blocked account cannot open a public game source', () => scenario('public', 'blocked-before'));
});
