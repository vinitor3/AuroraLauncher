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
const result = await call("Runtime.evaluate", {
  awaitPromise: true,
  returnByValue: true,
  expression: `
    (async () => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      const running = await invoke("list_running_instances");
      if (running.length) throw new Error("Execute o smoke desktop com Minecraft fechado");
      const nativeShortcutRegistered = await invoke("plugin:global-shortcut|is_registered", { shortcut: "Ctrl+Alt+/" });
      if (!nativeShortcutRegistered) throw new Error("Ctrl+Alt+/ não está registrado");
      const button = [...document.querySelectorAll("button")]
        .find((item) => item.textContent?.trim() === "Assistente");
      if (!button) throw new Error("Botão desktop do Assistente não encontrado");
      const panelVisible = () => Boolean(document.querySelector('[aria-label="Assistente Aurora"]'));
      const wasOpen = panelVisible();
      button.click();
      await new Promise((resolve) => setTimeout(resolve, 150));
      if (panelVisible() === wasOpen) throw new Error("O botão não alternou o painel desktop");
      button.click();
      await new Promise((resolve) => setTimeout(resolve, 150));
      if (panelVisible() !== wasOpen) throw new Error("O botão não restaurou a visibilidade do painel");
      return { nativeShortcutRegistered, buttonTogglesPanel: true, minecraftClosed: true,
        physicalShortcutPress: "PRECISA DE TESTE manual; callback coberto pelos testes unitários desktop" };
    })()
  `,
});
socket.close();
if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? "Falha no WebView");
console.log(JSON.stringify(result.result?.value));
