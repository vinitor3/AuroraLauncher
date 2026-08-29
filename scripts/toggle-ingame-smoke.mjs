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
  expression: `window.__TAURI_INTERNALS__.invoke("toggle_ipc_assistant")`,
});
socket.close();
if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? "Falha no WebView");
console.log(JSON.stringify({ deliveredToGame: result.result?.value }));
