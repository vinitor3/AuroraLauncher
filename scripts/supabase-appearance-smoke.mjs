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
      const originalFetch = window.fetch.bind(window);
      let capturedAuthorization = '';
      window.fetch = (input, init) => {
        const url = typeof input === 'string' ? input : input.url;
        if (url.includes('/v1/assistant')) {
          capturedAuthorization = new Headers(init?.headers).get('Authorization') ?? '';
        }
        return originalFetch(input, init);
      };
      let assistantButton;
      const readyAt = Date.now();
      while (!assistantButton && Date.now() - readyAt < 15000) {
        assistantButton = [...document.querySelectorAll('button')]
          .find((button) => button.textContent?.trim() === 'Assistente');
        if (!assistantButton) await new Promise((resolve) => setTimeout(resolve, 200));
      }
      if (!assistantButton) throw new Error('Sessão Aurora autenticada não ficou pronta');
      if (!document.querySelector('[aria-label="Assistente Aurora"]')) assistantButton.click();
      await new Promise((resolve) => setTimeout(resolve, 250));
      const input = document.querySelector('textarea[aria-label="Mensagem para o Assistente"]');
      const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
      setter.call(input, 'Responda apenas: ok');
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.form.requestSubmit();
      const authAt = Date.now();
      while (!capturedAuthorization && Date.now() - authAt < 10000) {
        await new Promise((resolve) => setTimeout(resolve, 100));
      }
      window.fetch = originalFetch;
      if (!capturedAuthorization) throw new Error('Token autenticado não foi encaminhado pelo launcher');
      const canvas = document.createElement('canvas');
      canvas.width = 64;
      canvas.height = 64;
      const context = canvas.getContext('2d');
      context.fillStyle = '#4c5fd7';
      context.fillRect(0, 0, 64, 64);
      const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'));
      const form = new FormData();
      form.set('kind', 'skin');
      form.set('file', blob, 'skin.png');
      const response = await originalFetch('https://aurora-api.aurora-edge-proxy.workers.dev/v1/appearance', {
        method: 'POST',
        headers: { Authorization: capturedAuthorization },
        body: form,
      });
      const payload = await response.json();
      if (!response.ok) return { uploadStatus: response.status, payload };
      const publicResponse = await originalFetch(payload.url, { cache: 'no-store' });
      const bytes = new Uint8Array(await publicResponse.arrayBuffer());
      return {
        uploadStatus: response.status,
        publicStatus: publicResponse.status,
        contentType: publicResponse.headers.get('content-type'),
        pngSignature: [...bytes.slice(0, 8)].map((value) => value.toString(16).padStart(2, '0')).join(''),
        url: payload.url,
      };
    })()
  `,
});
socket.close();
if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? "Falha no WebView");
console.log(JSON.stringify(result.result?.value, null, 2));
