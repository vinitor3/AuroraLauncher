// Explicit local GUI companion. It controls only disposable SDK/echo fixtures and
// the same ConnectManager test process; the desktop is operated through its UI.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { isAbsolute } from 'node:path';
import { stat } from 'node:fs/promises';
import { startSocialBindingLab } from './social-binding-server.mjs';
import { account, echo, manager, prepareHost, prepareGuest, rebind, socialFactory, tcpRoundtrip, waitSnapshot } from './social-desktop-fixtures.mjs';

if (process.env.AURORA_SOCIAL_DESKTOP_GUI !== '1') throw new Error('SOCIAL_GUI_OPT_IN_REQUIRED');
const binary = process.env.AURORA_SOCIAL_DESKTOP_TEST_BINARY, executable = process.env.AURORA_SOCIAL_DESKTOP_EXE;
for (const path of [binary, executable]) assert.ok(path && isAbsolute(path) && (await stat(path)).isFile(), 'reviewed absolute binary paths required');
const report = value => process.stdout.write(JSON.stringify(value) + '\n');
let lab, guiAccount, peerAccount, source, peer, gui, guiExited = false, guiExitCode, closing = false;
const input = createInterface({ input: process.stdin, terminal: false });
const watchdog = setTimeout(() => { report({ event: 'gui_fixture_timeout' }); input.close(); }, 1_200_000);
async function closePeer() { if (peer) { await peer.close(); peer = undefined; } }
async function publish(who, visibility) {
  await who.service.publishSession({ title: visibility === 'friends' ? 'Sessão dos amigos' : 'Sessão pública local', modpackName: 'Aurora Connect local',
    minecraftVersion: '1.21.1', loader: 'Fabric', imageUrl: null, players: 1, maxPlayers: 2, visibility, joinPolicy: 'open' });
  return who.sessionId();
}
async function clean() {
  if (closing) return; closing = true; clearTimeout(watchdog);
  let failed = false;
  const steps = [() => closePeer(), () => source?.close(), () => guiAccount?.service.closeSession(), () => peerAccount?.service.closeSession(),
    () => guiAccount?.close(), () => peerAccount?.close(), () => lab?.close(), () => rebind('127.0.0.1:43001'), () => rebind('127.0.0.1:42888')];
  for (const step of steps) { try { await step(); } catch { failed = true; } }
  const normalExit = guiExited && guiExitCode === 0;
  report({ event: 'gui_fixture_closed', cleanup: !failed, nativeApplicationExited: guiExited, nativeApplicationExitCode: guiExitCode ?? null, nativeApplicationNormalExit: normalExit, terminationObservation: 'exit_code_only' });
  if (failed || gui && !normalExit) process.exitCode = 1;
}
try {
  lab = await startSocialBindingLab({ enabled: true, port: 42888, admission: true });
  const factory = await socialFactory();
  guiAccount = await account(factory, 'gui89'); peerAccount = await account(factory, 'peer89'); source = await echo();
  await guiAccount.service.requestFriend(peerAccount.uid); await peerAccount.service.respondFriend(guiAccount.uid, 'accept');
  gui = spawn(executable, [], { windowsHide: true, stdio: 'ignore', env: { ...process.env,
    AURORA_SOCIAL_AUTHORITY_PIN: lab.admissionPublicKey, AURORA_SOCIAL_AUTHORITY_ISSUER: lab.issuerInstance } });
  gui.once('error', () => report({ event: 'gui_launch_failed' }));
  gui.once('exit', code => { guiExited = true; guiExitCode = code; report({ event: 'gui_application_exited', exitCode: code, terminationObservation: 'exit_code_only' }); });
  report({ event: 'gui_fixture_ready', nickname: guiAccount.nickname, peerNickname: peerAccount.nickname, localEmulatorOnly: true });
  for await (const raw of input) {
    if (raw.length > 128) throw new Error('GUI_FIXTURE_INPUT_TOO_LARGE');
    const [command, argument] = raw.trim().split(' ');
    try {
      if (command === 'host-public' || command === 'host-friends') {
        await closePeer();
        const visibility = command === 'host-public' ? 'public' : 'friends';
        const sessionId = await publish(peerAccount, visibility);
        peer = await manager(binary, lab); await prepareHost(peer, peerAccount, sessionId, visibility);
        report({ event: 'gui_peer_host_ready', visibility });
      } else if (command === 'probe') {
        assert.match(argument ?? '', /^127\.0\.0\.1:[0-9]+$/); await tcpRoundtrip(argument);
        report({ event: 'gui_tcp_passed', tcpBytesEachDirection: 1_048_576, minecraft: 'not_tested', voice: 'not_tested' });
      } else if (command === 'peer-status') {
        const s = await peer.ok('snapshot'); report({ event: 'gui_peer_state', phase: s.phase, stage: s.social?.stage, cleanupState: s.cleanupState, errorCode: s.errorCode });
      } else if (command === 'guest') {
        await closePeer();
        const sessionId = await guiAccount.sessionId();
        peer = await manager(binary, lab); await prepareGuest(peer, peerAccount, sessionId);
        const s = await waitSnapshot(peer, value => value.phase === 'guestConnected');
        await tcpRoundtrip(s.tcpAddress); report({ event: 'gui_host_tcp_passed', tcpBytesEachDirection: 1_048_576 });
      } else if (command === 'block') {
        await peerAccount.service.block(guiAccount.uid); report({ event: 'gui_peer_blocked' });
      } else if (command === 'close-peer') {
        await closePeer(); await peerAccount.service.closeSession(); report({ event: 'gui_peer_closed' });
      } else if (command === 'quit') { break; }
      else report({ event: 'gui_fixture_invalid_command' });
    } catch { report({ event: 'gui_fixture_operation_failed', command: ['host-public','host-friends','probe','peer-status','guest','block','close-peer'].includes(command) ? command : 'unknown' }); }
  }
} finally { await clean(); input.close(); }
