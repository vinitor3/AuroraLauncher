// Execute the actual desktop functions with deterministic browser/native adapters.
// These unit tests do not establish WebView, microphone, Edge TTS or Gemini runtime health.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";

const source = ts.createSourceFile("App.tsx", await readFile(new URL("../App.tsx", import.meta.url), "utf8"),
  ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);

function compile(text, dependencies) {
  const { outputText } = ts.transpileModule(`const subject = ${text};`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.React, jsxFactory: "createElement" },
  });
  return new Function(...Object.keys(dependencies), `${outputText}\nreturn subject;`)(...Object.values(dependencies));
}

function functionSource(name) {
  const declaration = source.statements.find((node) => ts.isFunctionDeclaration(node) && node.name?.text === name);
  assert.ok(declaration, `${name} must remain available to the desktop`);
  return declaration.getText(source);
}

test("Ctrl+Alt+/ alterna o painel em Pressed, ignora Released e dispensa qualquer comando do jogo", async () => {
  let effect;
  function visit(node) {
    if (ts.isCallExpression(node) && node.expression.getText(source) === "useEffect"
      && node.arguments[0]?.getText(source).includes('register("Ctrl+Alt+/"')) effect = node.arguments[0];
    ts.forEachChild(node, visit);
  }
  visit(source);
  assert.ok(effect, "O atalho desktop deve continuar registrado");
  let open = false;
  let callback;
  const notices = [];
  const runEffect = compile(effect.getText(source), {
    register: async (shortcut, handler) => { assert.equal(shortcut, "Ctrl+Alt+/"); callback = handler; },
    setAssistantOpen: (update) => { open = update(open); },
    setNotice: (value) => notices.push(value),
  });
  runEffect();
  callback({ state: "Released" });
  assert.equal(open, false);
  callback({ state: "Pressed" });
  assert.equal(open, true);
  callback({ state: "Pressed" });
  assert.equal(open, false);
  assert.deepEqual(notices, []);
  const failure = compile(effect.getText(source), {
    register: async () => { throw new Error("shortcut unavailable"); },
    setAssistantOpen: () => assert.fail("falha no registro não deve alternar o painel"),
    setNotice: (value) => notices.push(value),
  });
  failure();
  await Promise.resolve();
  assert.match(notices.at(-1), /Ctrl\+Alt\+\/ indisponível/);
});

test("polling preserva processos e renderiza o contrato serializado do Core sem tratar pedidos legados", async () => {
  let effect;
  let card;
  function visit(node) {
    if (ts.isCallExpression(node) && node.expression.getText(source) === "useEffect"
      && node.arguments[0]?.getText(source).includes('"poll_ipc_events"')) effect = node.arguments[0];
    if (ts.isJsxExpression(node) && node.expression?.getText(source).startsWith("coreConnection &&")) card = node.expression;
    ts.forEachChild(node, visit);
  }
  visit(source);
  assert.ok(effect);
  assert.ok(card);
  const events = [
    [{ processId: 123, event: { kind: "connected", loader: "fabric", minecraft_version: "1.20.1" } }],
    [{ processId: 123, event: { kind: "moduleEvent", name: "skins.ready", payload: {} } },
      { processId: 123, event: { kind: "assistantRequest", requestId: "retired", message: "Ignore me" } }],
    [{ processId: 123, event: { kind: "disconnected" } }],
  ];
  const connections = [];
  const processes = [];
  let tick;
  let cleared = false;
  const run = compile(effect.getText(source), {
    invoke: async (command) => {
      if (command === "list_running_instances") return [{ instanceId: "qa", processId: 123 }];
      if (command === "poll_ipc_events") return events.shift();
      assert.fail(`Comando inesperado: ${command}`);
    },
    window: {
      setInterval: (callback, interval) => { assert.equal(interval, 1_000); tick = callback; return 7; },
      clearInterval: (id) => { assert.equal(id, 7); cleared = true; },
    },
    setRunningInstances: (value) => processes.push(value),
    setCoreConnection: (value) => connections.push(value),
  });
  const render = compile(`(coreConnection) => (${card.getText(source)})`, {
    createElement: (type, props, ...children) => ({ type, props, children }),
  });
  const stop = run();
  tick();
  await new Promise(setImmediate);
  assert.match(JSON.stringify(render(connections.at(-1))), /Conectado: fabric 1\.20\.1/);
  tick();
  await new Promise(setImmediate);
  assert.equal(connections.length, 1, "evento de módulo ou ponte retirada não substitui o diagnóstico");
  tick();
  await new Promise(setImmediate);
  assert.equal(processes.length, 4, "processos são atualizados inicialmente e em cada tick");
  assert.match(JSON.stringify(render(connections.at(-1))), /Aurora Core desconectado/);
  stop();
  assert.equal(cleared, true);
});

function speechHarness({ mimeType = "audio/mpeg", audioBase64 = btoa("a".repeat(128)), loadError = false, playError = false } = {}) {
  const revoked = [];
  let audio;
  const play = compile(functionSource("playAuroraSpeech"), {
    invoke: async (command, args) => {
      assert.equal(command, "synthesize_speech");
      assert.deepEqual(args, { text: "Olá Aurora" });
      return { mimeType, audioBase64, boundaries: [] };
    },
    window: { atob },
    URL: { createObjectURL: () => "blob:desktop-speech", revokeObjectURL: (url) => revoked.push(url) },
    Audio: class extends EventTarget {
      constructor() { super(); audio = this; }
      load() {
        queueMicrotask(() => {
          if (loadError) this.error = { code: 4, message: "unsupported media" };
          this.dispatchEvent(new Event(loadError ? "error" : "canplay"));
        });
      }
      async play() { if (playError) throw new Error("playback denied"); }
    },
  });
  return { play: () => play("Olá Aurora"), revoked, audio: () => audio };
}

test("fala desktop usa synthesize_speech e libera o áudio ao pausar/encerrar", async () => {
  const harness = speechHarness();
  const audio = await harness.play();
  assert.equal(audio, harness.audio());
  assert.equal(audio.src, "blob:desktop-speech");
  audio.dispatchEvent(new Event("pause"));
  audio.dispatchEvent(new Event("ended"));
  assert.deepEqual(harness.revoked, ["blob:desktop-speech"]);
});

test("fala desktop trata áudio inválido e falhas de mídia/reprodução", async () => {
  await assert.rejects(speechHarness({ mimeType: "text/plain" }).play(), /incompatível/);
  await assert.rejects(speechHarness({ audioBase64: "!" }).play(), /corrompido/);
  await assert.rejects(speechHarness({ audioBase64: btoa("short") }).play(), /vazio/);
  for (const configuration of [{ loadError: true }, { playError: true }]) {
    const harness = speechHarness(configuration);
    await assert.rejects(harness.play(), /unsupported media|playback denied/);
    assert.deepEqual(harness.revoked, ["blob:desktop-speech"]);
  }
});

function panelHarness({ recognitionAvailable = true, ask = async () => "Resposta desktop" } = {}) {
  const slots = [];
  const cleanups = [];
  const effects = [];
  let slot = 0;
  let effectIndex = 0;
  let recognition;
  let spoken;
  let audioPaused = 0;
  const createElement = (type, props, ...children) => ({ type, props: props ?? {}, children: children.flat(Infinity).filter(Boolean) });
  const useState = (initial) => {
    const index = slot++;
    if (!(index in slots)) slots[index] = typeof initial === "function" ? initial() : initial;
    return [slots[index], (value) => { slots[index] = typeof value === "function" ? value(slots[index]) : value; }];
  };
  const Panel = compile(functionSource("AssistantPanel"), {
    createElement, useState,
    useRef: (initial) => useState({ current: initial })[0],
    useEffect: (effect, dependencies) => {
      const index = effectIndex++;
      if (!effects[index] || dependencies.some((value, i) => value !== effects[index][i])) {
        effects[index] = dependencies;
        const cleanup = effect();
        if (cleanup) cleanups.push(cleanup);
      }
    },
    window: recognitionAvailable ? { SpeechRecognition: class {
      constructor() { recognition = this; }
      start() { this.started = true; }
      stop() { this.stopped = true; this.onend?.(); }
    } } : {},
    askAuroraAssistant: ask,
    playAuroraSpeech: async (text) => { spoken = text; return { pause: () => { audioPaused += 1; } }; },
  });
  const render = () => { slot = 0; effectIndex = 0; return Panel({ services: {}, username: "QA", onClose: () => {} }); };
  function find(tree, predicate) {
    if (predicate(tree)) return tree;
    return tree.children?.map((child) => typeof child === "object" ? find(child, predicate) : null).find(Boolean);
  }
  return {
    render,
    control: (label) => find(render(), (node) => node.props?.["aria-label"] === label),
    form: () => find(render(), (node) => node.type === "form"),
    text: () => JSON.stringify(render()),
    recognition: () => recognition,
    spoken: () => spoken,
    audioPaused: () => audioPaused,
    unmount: () => cleanups.forEach((cleanup) => cleanup()),
  };
}

test("painel mantém conversa, transcrição, mute e reprodução sem instância Minecraft", async () => {
  const panel = panelHarness({ ask: async (_services, message) => { assert.equal(message, "Pergunta por voz"); return "Resposta desktop"; } });
  panel.control("Usar microfone").props.onClick();
  assert.equal(panel.recognition().started, true);
  assert.equal(panel.recognition().lang, "pt-BR");
  panel.recognition().onresult({ results: [[{ transcript: "Pergunta por voz" }]] });
  assert.equal(panel.control("Mensagem para o Assistente").props.value, "Pergunta por voz");
  await panel.form().props.onSubmit({ preventDefault() {} });
  assert.match(panel.text(), /Pergunta por voz/);
  assert.match(panel.text(), /Resposta desktop/);
  assert.equal(panel.spoken(), "Resposta desktop");
  panel.control("Mutar áudio").props.onClick();
  assert.equal(panel.recognition().stopped, true);
  assert.equal(panel.control("Usar microfone").props.disabled, true);
  assert.equal(panel.audioPaused(), 1);
  panel.control("Ativar áudio").props.onClick();
  assert.equal(panel.control("Usar microfone").props.disabled, false);
  panel.unmount();
  assert.equal(panel.audioPaused(), 2);
});

test("painel mantém mensagens de falha de rede e microfone", async () => {
  const panel = panelHarness({ ask: async () => { throw new Error("Falha de rede tratada"); } });
  panel.control("Mensagem para o Assistente").props.onChange({ target: { value: "Olá" } });
  await panel.form().props.onSubmit({ preventDefault() {} });
  assert.match(panel.text(), /Falha de rede tratada/);
  panel.control("Usar microfone").props.onClick();
  panel.recognition().onerror();
  assert.match(panel.text(), /Não consegui compreender o áudio/);
  const unavailable = panelHarness({ recognitionAvailable: false });
  unavailable.control("Usar microfone").props.onClick();
  assert.match(unavailable.text(), /microfone por voz não está disponível/);
});
