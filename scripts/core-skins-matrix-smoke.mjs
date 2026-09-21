// Run only against an isolated launcher QA data root. Each run creates a fresh
// disposable instance; no existing instance is selected or mutated.
import { randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";

const [minecraftVersion, loader] = process.argv.slice(2);
const matrix = JSON.parse(readFileSync(new URL("./active-mod-matrix.json", import.meta.url), "utf8"));
const buildTarget = matrix.targets.find(item => item.minecraft === minecraftVersion && item.loader === loader);
if (!buildTarget) {
  throw new Error("Uso: node core-skins-matrix-smoke.mjs <versão ativa> <fabric|forge>");
}
const id = `qa-core-skins-${minecraftVersion}-${loader}-${randomUUID().slice(0, 8)}`;
const targets = await (await fetch("http://127.0.0.1:9222/json")).json();
const target = targets.find((item) => item.title === "Aurora Smart Launcher");
if (!target?.webSocketDebuggerUrl) throw new Error("WebView Aurora não encontrado");
const socket = new WebSocket(target.webSocketDebuggerUrl);
const pending = new Map();
let sequence = 0;
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  const operation = pending.get(message.id);
  if (!operation) return;
  pending.delete(message.id);
  message.error ? operation.reject(new Error(message.error.message)) : operation.resolve(message.result);
};
await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
function call(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++sequence;
    pending.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params }));
  });
}
await call("Runtime.enable");
const expression = `
  (async () => {
    const internal = window.__TAURI_INTERNALS__;
    const originalInvoke = internal.invoke;
    const observedEvents = [];
    // Observe both the application poller and this QA poller so they do not race
    // to consume the one Core handshake event. Restore the bridge in all cases.
    internal.invoke = async function(command, ...args) {
      const result = await originalInvoke.call(this, command, ...args);
      if (command === 'poll_ipc_events' && Array.isArray(result)) observedEvents.push(...result);
      return result;
    };
    try {
    const invoke = (...args) => internal.invoke(...args);
    const id = ${JSON.stringify(id)};
    const minecraftVersion = ${JSON.stringify(minecraftVersion)};
    const loader = ${JSON.stringify(loader)};
    await invoke('create_instance', { id });
    const install = loader === 'fabric'
      ? await invoke('install_fabric', { id, minecraftVersion })
      : await invoke('install_forge', {
          id,
          minecraftVersion,
          forgeVersion: ${JSON.stringify(buildTarget.loaderVersion)},
        });
    const runtime = await invoke('ensure_java', { minecraftVersion });
    const appearanceUrl = await invoke('validate_appearance_url', {
      url: 'https://minotar.net/skin/vinitor3',
      kind: 'skin',
    });
    const launch = await invoke('launch_instance', {
      id,
      versionId: install.versionId,
      minecraftVersion,
      javaExecutable: runtime.executable,
      nickname: 'AuroraQA',
      skinUrl: appearanceUrl,
      capeUrl: null,
      skinModel: 'classic',
    });
    if (!launch.coreInstalled || !launch.skinsInstalled) throw new Error('Core/Skins não foram instalados');
    const deadline = Date.now() + 120000;
    let handshake;
    while (!handshake && Date.now() < deadline) {
      await new Promise((resolve) => setTimeout(resolve, 500));
      await invoke('poll_ipc_events');
      handshake = observedEvents.find(item => item.processId === launch.processId
        && item.event.kind === 'connected'
        && item.event.loader === loader && item.event.minecraft_version === minecraftVersion);
    }
    if (!handshake) throw new Error('Handshake autenticado do Core não foi observado no prazo');
    return {
      id,
      minecraftVersion,
      loader,
      versionId: install.versionId,
      processId: launch.processId,
      coreInstalled: launch.coreInstalled,
      skinsInstalled: launch.skinsInstalled,
      connected: true,
    };
    } catch (error) {
      return { id: ${JSON.stringify(id)}, error: String(error) };
    } finally {
      internal.invoke = originalInvoke;
    }
  })()
`;
const result = await call("Runtime.evaluate", { awaitPromise: true, returnByValue: true, expression });
socket.close();
if (result.exceptionDetails) {
  throw new Error(result.exceptionDetails.exception?.description
    ?? result.exceptionDetails.text
    ?? JSON.stringify(result.exceptionDetails));
}
console.log(JSON.stringify(result.result?.value, null, 2));
if (result.result?.value?.error) process.exitCode = 1;
