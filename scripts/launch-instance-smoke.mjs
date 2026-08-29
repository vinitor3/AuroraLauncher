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
      const readyAt = Date.now();
      while (!document.body.innerText.includes("fabulously-optimized-1.20.1") && Date.now() - readyAt < 15000) {
        await new Promise((resolve) => setTimeout(resolve, 250));
      }
      const instanceLabel = [...document.querySelectorAll("*")]
        .find((element) => element.children.length === 0 && element.textContent?.trim() === "fabulously-optimized-1.20.1");
      const card = instanceLabel?.closest("article, section, div");
      const select = [...(card?.querySelectorAll("button") ?? [])]
        .find((button) => button.textContent?.trim() === "Selecionar")
        ?? [...document.querySelectorAll("button")].find((button) => button.textContent?.trim() === "Selecionar");
      if (!select) throw new Error("Botão Selecionar não encontrado");
      select.click();
      await new Promise((resolve) => setTimeout(resolve, 400));
      const start = [...document.querySelectorAll("button")]
        .find((button) => button.textContent?.includes("Iniciar") && !button.disabled);
      if (!start) throw new Error("Botão Iniciar não ficou disponível");
      start.click();
      await new Promise((resolve) => setTimeout(resolve, 700));
      return {
        text: document.body.innerText.slice(-5000),
        inputs: [...document.querySelectorAll("input, select")].map((input) => ({
          label: input.getAttribute("aria-label") ?? input.previousElementSibling?.textContent?.trim() ?? "",
          name: input.getAttribute("name") ?? "",
          type: input.getAttribute("type") ?? input.tagName.toLowerCase(),
          value: input.getAttribute("type") === "password" ? "[redacted]" : input.value,
        })),
        buttons: [...document.querySelectorAll("button")].map((button) => button.textContent?.trim()).filter(Boolean),
      };
    })()
  `,
});
socket.close();
if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? "Falha no WebView");
console.log(JSON.stringify(result.result?.value, null, 2));
