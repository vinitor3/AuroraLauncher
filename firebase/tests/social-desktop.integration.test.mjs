import assert from 'node:assert/strict';
import { test } from 'node:test';
import { isAbsolute } from 'node:path';
import { stat } from 'node:fs/promises';
import { startSocialBindingLab } from './social-binding-server.mjs';
import { account, activeTcp, delay, echo, manager, prepareHost, prepareGuest, rebind, socialFactory, tcpRoundtrip, waitSnapshot } from './social-desktop-fixtures.mjs';

const enabled = process.env.AURORA_SOCIAL_DESKTOP_INTEGRATION === '1';
test('private directed invitation joins two native managers and revocation closes the active TCP flow', { skip: !enabled, timeout: 90_000 }, async t => {
  const binary = process.env.AURORA_SOCIAL_DESKTOP_TEST_BINARY;
  assert.ok(binary && isAbsolute(binary), 'absolute reviewed desktop test binary required');
  assert.ok((await stat(binary)).isFile());
  let lab, host, guest, third, source, hostClient, guestClient, active, bridge, primary, cleanupFailure;
  const started = performance.now();
  try {
    const factory = await socialFactory();
    lab = await startSocialBindingLab({ enabled: true, port: 42888, admission: true, privateInvitations: true });
    host = await account(factory, 'pvhost'); guest = await account(factory, 'pvgest'); third = await account(factory, 'pvothr');
    source = await echo();
    await host.service.publishSession({ title: 'Private local invitation', modpackName: 'Native private fixture', minecraftVersion: '1.21.1', loader: 'Fabric',
      imageUrl: null, players: 1, maxPlayers: 2, visibility: 'private', joinPolicy: 'open' });
    const sessionId = await host.sessionId();
    assert.ok(host.service.privateInvitations); assert.ok(guest.service.privateInvitations);
    await host.service.privateInvitations.invite(sessionId, guest.uid);
    const invitedAt = performance.now();
    const inbox = await guest.service.privateInvitations.snapshot();
    assert.equal(inbox.incoming.length, 1); assert.equal(inbox.incoming[0].session.id, sessionId);
    assert.equal((await third.service.privateInvitations.snapshot()).incoming.length, 0);
    hostClient = await manager(binary, lab); guestClient = await manager(binary, lab);
    assert.equal(hostClient.ready.capabilities.privateInvitations, true);
    await prepareHost(hostClient, host, sessionId, 'private');
    const denied = await fetch(lab.origin + '/v1/social/context', { method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${await third.token()}` }, body: JSON.stringify({ sessionId }) });
    assert.equal(denied.status, 403); assert.equal((await denied.json()).code, 'context_unavailable'); assert.equal(source.connections, 0);
    await prepareGuest(guestClient, guest, sessionId);
    const connected = await waitSnapshot(guestClient, value => value.phase === 'guestConnected' && value.social?.stage === 'connected');
    bridge = connected.tcpAddress; assert.match(bridge, /^127\.0\.0\.1:\d+$/);
    await tcpRoundtrip(bridge); active = await activeTcp(bridge);
    assert.equal(active.isOpen, true); assert.ok(connected.social.expiresAtMs - Date.now() > 10_000);
    const closed = active.withdrawn(); closed.catch(() => {});
    const revokeAt = performance.now(); await host.service.privateInvitations.revoke(sessionId, guest.uid); await closed;
    const [hostClosed, guestClosed] = await Promise.all([
      waitSnapshot(hostClient, value => ['closed', 'failed'].includes(value.phase) && value.cleanupState === 'complete'),
      waitSnapshot(guestClient, value => ['closed', 'failed'].includes(value.phase) && value.cleanupState === 'complete'),
    ]);
    assert.equal(hostClosed.transportState, 'closed'); assert.equal(guestClosed.tcpAddress, null); await rebind(bridge);
    const withdrawalMs = Math.round(performance.now() - revokeAt), connectionsAfterWithdrawal = source.connections;
    assert.equal((await guest.service.privateInvitations.snapshot()).incoming.length, 0);
    // Reissuing the account authorization must not restart a terminal native attempt.
    await delay(Math.max(0, 2100 - (performance.now() - invitedAt)));
    await host.service.privateInvitations.invite(sessionId, guest.uid);
    assert.equal((await guest.service.privateInvitations.snapshot()).incoming.length, 1);
    const unchanged = await guestClient.ok('snapshot'); assert.equal(unchanged.operationId, guestClosed.operationId);
    assert.ok(['closed', 'failed'].includes(unchanged.phase)); assert.equal(unchanged.tcpAddress, null); assert.equal(source.connections, connectionsAfterWithdrawal);
    t.diagnostic(JSON.stringify({ case: 'private-directed-nonfriend', tcpBytesEachDirection: 1_048_576, thirdAccountDenied: true,
      activeTcpWithdrawn: true, automaticCleanup: true, tcpRebind: true, reinvitationDidNotRevive: true, withdrawalMs,
      elapsedMs: Math.round(performance.now() - started), minecraft: 'not_tested', voice: 'not_tested', externalNetwork: 'not_tested' }));
  } catch (error) { primary = error; }
  finally {
    active?.close();
    for (const cleanup of [guestClient && (() => guestClient.close()), hostClient && (() => hostClient.close()), source && (() => source.close()),
      third && (() => third.close()), guest && (() => guest.close()), host && (() => host.close()), lab && (() => lab.close())].filter(Boolean)) {
      try { await cleanup(); } catch (error) { cleanupFailure ??= error; }
    }
    if (source) { try { await rebind('127.0.0.1:43001'); } catch (error) { cleanupFailure ??= error; } }
  }
  if (primary && cleanupFailure) throw new AggregateError([primary, cleanupFailure], 'PRIVATE_SCENARIO_AND_CLEANUP_FAILED');
  if (cleanupFailure) throw cleanupFailure;
  if (primary) throw primary;
});
test('launcher ConnectManager joins and launches through authenticated local social authority', { skip: !enabled, timeout: 240_000 }, async t => {
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
  await t.test('90 discovered owned source joins through the broker and exit closes both peers', async () => {
    let lab, host, guest, hostClient, guestClient, bridge, primary, cleanupFailure;
    const started = performance.now();
    try {
      lab = await startSocialBindingLab({ enabled: true, port: 42888, admission: true });
      host = await account(factory, 'src90'); guest = await account(factory, 'gst90');
      await host.service.publishSession({ title: 'Owned source fixture', modpackName: 'Connect discovery test', minecraftVersion: '1.21.1', loader: 'Fabric',
        imageUrl: null, players: 1, maxPlayers: 2, visibility: 'public', joinPolicy: 'open' });
      const sessionId = await host.sessionId();
      hostClient = await manager(binary, lab); guestClient = await manager(binary, lab);
      const source = await hostClient.ok('discoverFixture');
      assert.match(source.target, /^127\.0\.0\.1:\d+$/);
      assert.notEqual(source.target, '127.0.0.1:43001', 'source must be discovered on an ephemeral port');
      const reservation = await hostClient.ok('reserveDiscovered', { selectionId: source.selectionId, request: {
        accountEpoch: hostClient.ready.capabilities.accountEpoch, accountId: host.uid, sessionId, visibility: 'public',
      } });
      await hostClient.ok('authenticate', { ...reservation, token: await host.token() });
      const review = await waitSnapshot(hostClient, value => value.social?.stage === 'hostReview');
      assert.equal(review.social.gameTarget, source.target);
      await hostClient.ok('confirm', { ...reservation, generation: review.generation, digest: review.social.digest });
      await waitSnapshot(hostClient, value => value.social?.stage === 'waitingGuest');
      await prepareGuest(guestClient, guest, sessionId);
      const connected = await waitSnapshot(guestClient, value => value.phase === 'guestConnected');
      bridge = connected.tcpAddress;
      const status = await guestClient.ok('probeStatus');
      assert.equal(status.requestedProtocol, 767); assert.equal(status.reportedProtocol, 767);
      assert.equal(status.protocolMatchesRequested, true);
      const remaining = connected.social.expiresAtMs - Date.now();
      assert.ok(remaining > 10_000, 'source withdrawal must precede ordinary expiry');
      const stopped = await hostClient.ok('stopFixture');
      assert.equal(stopped.cleanup, true, 'owned source child must exit normally');
      const [closedHost, closedGuest] = await Promise.all([
        waitSnapshot(hostClient, value => ['closed', 'failed'].includes(value.phase) && value.cleanupState === 'complete'),
        waitSnapshot(guestClient, value => ['closed', 'failed'].includes(value.phase) && value.cleanupState === 'complete'),
      ]);
      assert.equal(closedHost.errorCode, 'connect_source_changed');
      assert.equal(closedGuest.tcpAddress, null);
      await rebind(bridge);
      t.diagnostic(JSON.stringify({ case: 'public-owned-source', sourceDiscovered: true, statusAndMatchingPong: true,
        sourceExitClosedBothPeers: true, tcpRebind: true, remainingSessionMs: remaining,
        elapsedMs: Math.round(performance.now() - started), source: 'synthetic_owned_child', minecraft: 'not_tested', voice: 'not_tested' }));
    } catch (error) { primary = error; }
    finally {
      for (const cleanup of [guestClient && (() => guestClient.close()), hostClient && (() => hostClient.close()),
        guest && (() => guest.close()), host && (() => host.close()), lab && (() => lab.close())].filter(Boolean)) {
        try { await cleanup(); } catch (error) { cleanupFailure ??= error; }
      }
    }
    if (primary && cleanupFailure) throw new AggregateError([primary, cleanupFailure], 'DISCOVERY_AND_CLEANUP_FAILED');
    if (cleanupFailure) throw cleanupFailure;
    if (primary) throw primary;
  });
  async function socialLaunch(ending) {
    let lab, host, guest, source, hostClient, guestClient, primary, cleanupFailure;
    const started = performance.now();
    async function poll(command, args, predicate) {
      const deadline = performance.now() + 8_000;
      let value;
      while (performance.now() < deadline) {
        value = await guestClient.ok(command, args);
        if (predicate(value)) return value;
        await delay(40);
      }
      throw new Error(`READY_FIXTURE_TIMEOUT:${command}:${value?.phase ?? 'no-phase'}:${value?.errorCode ?? 'none'}`);
    }
    try {
      lab = await startSocialBindingLab({ enabled: true, port: 42888, admission: true });
      host = await account(factory, 'hst92'); guest = await account(factory, 'gst92'); source = await echo();
      await host.service.publishSession({ title: 'Social launch fixture', modpackName: 'Connect native launch test', minecraftVersion: '1.21.1', loader: 'Fabric',
        imageUrl: null, players: 1, maxPlayers: 2, visibility: 'public', joinPolicy: 'open' });
      const sessionId = await host.sessionId();
      hostClient = await manager(binary, lab); guestClient = await manager(binary, lab);
      await prepareHost(hostClient, host, sessionId, 'public');
      await prepareGuest(guestClient, guest, sessionId);
      const connected = await waitSnapshot(guestClient, value => value.phase === 'guestConnected');
      const operationId = connected.operationId, bridge = connected.tcpAddress;
      assert.equal(connected.social.readyGameLaunchAvailable, true);
      const hold = ending === 'held-prepare-stop' ? 'prepare' : ending === 'started-stop' ? 'none' : 'spawn';
      const preparing = await guestClient.ok('readyPrepare', { operationId, hold });
      assert.equal(preparing.phase, 'preparing');
      if (ending === 'held-prepare-stop') {
        await poll('readyFixture', {}, value => value.prepareEntered);
        const stopped = await guestClient.ok('readyStopDetached', { operationId });
        assert.equal(stopped.pending, true, 'lost stop waiter must leave real work owned');
        const closing = await waitSnapshot(guestClient, value => value.transportState === 'closed');
        assert.equal(closing.cleanupState, 'pending');
        await rebind(bridge);
        assert.equal((await guestClient.ok('readyFixture')).spawns, 0);
        const busy = await guestClient.call('reserve', { request: { accountEpoch: guestClient.ready.capabilities.accountEpoch,
          accountId: guest.uid, sessionId, role: 'guest', gameTarget: null, visibility: null } });
        assert.equal(busy.ok, false); assert.equal(busy.code, 'connect_busy');
        await guestClient.ok('readyRelease');
      } else {
        const reviewed = await poll('readySnapshot', { operationId }, value => value.phase === 'reviewed');
        assert.equal(reviewed.review.gameAddress, bridge);
        assert.equal(reviewed.review.minecraftVersion, '1.21.1');
        const confirmed = await guestClient.ok('readyConfirm', { operationId, planId: reviewed.review.planId, digest: reviewed.review.digest });
        assert.equal(confirmed.phase, 'starting');
        if (ending === 'started-stop') {
          await poll('readySnapshot', { operationId }, value => value.phase === 'started');
          const fixture = await guestClient.ok('readyFixture');
          assert.equal(fixture.spawns, 1); assert.equal(fixture.childAlive, true);
          await guestClient.ok('stop', { operationId });
          const after = await guestClient.ok('readyFixture');
          assert.equal(after.spawns, 1); assert.equal(after.childAlive, true, 'Connect stop must preserve the already registered game child');
        } else {
          await poll('readyFixture', {}, value => value.spawnEntered);
          if (ending === 'held-spawn-epoch') {
            const epoch = await guestClient.ok('readyAdvanceEpoch');
            assert.ok(epoch.epoch > guestClient.ready.capabilities.accountEpoch);
            assert.equal(epoch.originalCancelled, false);
          } else if (ending === 'held-spawn-block') {
            await host.service.block(guest.uid);
            await waitSnapshot(guestClient, value => value.transportState === 'closed');
            await rebind(bridge);
          } else throw new Error('UNKNOWN_READY_ENDING');
          await guestClient.ok('readyRelease');
          await poll('readySnapshot', { operationId }, value => ['failed', 'cancelled'].includes(value.phase) || !!value.errorCode);
          const rejected = await guestClient.ok('readyFixture');
          assert.equal(rejected.spawns, 0);
          if (ending === 'held-spawn-epoch') assert.equal(rejected.epochRejectedBeforeSpawn, true);
          await guestClient.ok('stop', { operationId });
        }
      }
      await waitSnapshot(guestClient, value => ['closed', 'failed'].includes(value.phase) && value.cleanupState === 'complete');
      await rebind(bridge);
      const fixture = await guestClient.ok('readyFixture');
      t.diagnostic(JSON.stringify({ case: `social-ready-${ending}`, originalSocialSource: true, nativeSpawnCount: fixture.spawns,
        registeredChildPreserved: ending === 'started-stop' ? fixture.childAlive : null, tcpRebind: true,
        elapsedMs: Math.round(performance.now() - started), process: 'native_test_fixture', minecraft: 'not_tested', voice: 'not_tested' }));
    } catch (error) { primary = error; }
    finally {
      // Release the same bounded fixture before quit, even if an assertion failed.
      if (guestClient) { try { await guestClient.ok('readyRelease'); } catch (error) { cleanupFailure ??= error; } }
      for (const cleanup of [guestClient && (() => guestClient.close()), hostClient && (() => hostClient.close()), source && (() => source.close()),
        guest && (() => guest.close()), host && (() => host.close()), lab && (() => lab.close())].filter(Boolean)) {
        try { await cleanup(); } catch (error) { cleanupFailure ??= error; }
      }
      if (source) { try { await rebind('127.0.0.1:43001'); } catch (error) { cleanupFailure ??= error; } }
    }
    if (primary && cleanupFailure) throw new AggregateError([primary, cleanupFailure], 'READY_AND_CLEANUP_FAILED');
    if (cleanupFailure) throw cleanupFailure;
    if (primary) throw primary;
  }
  await t.test('92 social source launches an owned child and Connect stop preserves it', () => socialLaunch('started-stop'));
  await t.test('92 account epoch fences the final held social spawn', () => socialLaunch('held-spawn-epoch'));
  await t.test('92 lost stop waiter releases social transport before held preparation', () => socialLaunch('held-prepare-stop'));
  await t.test('92 blocking a peer rejects a held social spawn', () => socialLaunch('held-spawn-block'));
  await t.test('92 expired original continuity rejects launch and a newer signed lease cannot revive it', async () => {
    let lab, host, guest, source, hostClient, guestClient, primary, cleanupFailure;
    const started = performance.now();
    try {
      lab = await startSocialBindingLab({ enabled: true, port: 42888, admission: true });
      host = await account(factory, 'hst92'); guest = await account(factory, 'gst92'); source = await echo();
      await host.service.publishSession({ title: 'Original continuity fixture', modpackName: 'Connect native authority test', minecraftVersion: '1.21.1', loader: 'Fabric',
        imageUrl: null, players: 1, maxPlayers: 2, visibility: 'public', joinPolicy: 'open' });
      const sessionId = await host.sessionId();
      hostClient = await manager(binary, lab); guestClient = await manager(binary, lab);
      await prepareHost(hostClient, host, sessionId, 'public');
      const reservation = await guestClient.ok('reserve', { request: { accountEpoch: guestClient.ready.capabilities.accountEpoch,
        accountId: guest.uid, sessionId, role: 'guest', gameTarget: null, visibility: null } });
      await guestClient.ok('guardProbeArm', { operationId: reservation.operationId });
      await guestClient.ok('authenticate', { ...reservation, token: await guest.token() });
      const connected = await waitSnapshot(guestClient, value => value.phase === 'guestConnected');
      // Do not poll snapshot while the original continuity naturally expires:
      // inspection must prove expiry is causal while transport and grant survive.
      const proof = await guestClient.ok('guardProbeInspect');
      assert.equal(proof.initialCallbackCalls, 1);
      for (const name of ['originalLeaseExpired', 'grantCurrent', 'invitationCurrent', 'guestAlive', 'bridgeAlive',
        'newerValidatedLease', 'lateLeaseCurrent', 'lateUpdateRejected', 'terminalRejected', 'sequenceUnchanged', 'terminal']) {
        assert.equal(proof[name], true, `original continuity oracle: ${name}`);
      }
      assert.equal(proof.originalCancelled, false);
      assert.equal(proof.callbackCalls, 0);
      await guestClient.ok('guardProbeRelease');
      await waitSnapshot(guestClient, value => ['closed', 'failed'].includes(value.phase) && value.cleanupState === 'complete');
      await rebind(connected.tcpAddress);
      t.diagnostic(JSON.stringify({ case: 'social-ready-original-continuity-expiry', originalLeaseExpired: true,
        transportAndGrantCurrentAtRefusal: true, freshSignedLeaseCannotRevive: true, launchCallbackCalls: 0, tcpRebind: true,
        elapsedMs: Math.round(performance.now() - started), minecraft: 'not_tested', voice: 'not_tested' }));
    } catch (error) { primary = error; }
    finally {
      if (guestClient) { try { await guestClient.ok('guardProbeRelease'); } catch (error) { cleanupFailure ??= error; } }
      for (const cleanup of [guestClient && (() => guestClient.close()), hostClient && (() => hostClient.close()), source && (() => source.close()),
        guest && (() => guest.close()), host && (() => host.close()), lab && (() => lab.close())].filter(Boolean)) {
        try { await cleanup(); } catch (error) { cleanupFailure ??= error; }
      }
      if (source) { try { await rebind('127.0.0.1:43001'); } catch (error) { cleanupFailure ??= error; } }
    }
    if (primary && cleanupFailure) throw new AggregateError([primary, cleanupFailure], 'CONTINUITY_AND_CLEANUP_FAILED');
    if (cleanupFailure) throw cleanupFailure;
    if (primary) throw primary;
  });
});
