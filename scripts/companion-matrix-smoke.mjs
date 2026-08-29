const [minecraftVersion, loader] = process.argv.slice(2);
if (!minecraftVersion || !["fabric", "forge"].includes(loader)) {
  throw new Error("Uso: node companion-matrix-smoke.mjs <versão> <fabric|forge>");
}
const forgeVersions = {
  "1.12.2": "14.23.5.2860",
  "1.16.5": "36.2.42",
  "1.19.2": "43.4.16",
  "1.20.1": "47.4.23",
  "1.21.1": "52.0.57",
};
const id = `phase1-smoke-${minecraftVersion}-${loader}`;
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
    try {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const id = ${JSON.stringify(id)};
    const minecraftVersion = ${JSON.stringify(minecraftVersion)};
    const loader = ${JSON.stringify(loader)};
    await invoke('create_instance', { id });
    const install = loader === 'fabric'
      ? await invoke('install_fabric', { id, minecraftVersion })
      : await invoke('install_forge', {
          id,
          minecraftVersion,
          forgeVersion: ${JSON.stringify(forgeVersions[minecraftVersion] ?? "")},
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
      nickname: 'vinitor3',
      skinUrl: appearanceUrl,
      capeUrl: null,
      skinModel: 'classic',
    });
    const deadline = Date.now() + 120000;
    let connected = false;
    while (!connected && Date.now() < deadline) {
      await new Promise((resolve) => setTimeout(resolve, 500));
      connected = await invoke('toggle_ipc_assistant');
    }
    if (connected) {
      await new Promise((resolve) => setTimeout(resolve, 1200));
      await invoke('toggle_ipc_assistant');
    }
    return {
      id,
      minecraftVersion,
      loader,
      versionId: install.versionId,
      processId: launch.processId,
      connected,
    };
    } catch (error) {
      return { id: ${JSON.stringify(id)}, error: String(error) };
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
