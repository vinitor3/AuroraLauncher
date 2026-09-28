// Execute the actual desktop functions with deterministic browser/native adapters.
// These unit tests do not establish WebView, microphone, Edge TTS or Gemini runtime health.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";
import { AI_ERRORS, publicAiError } from "../../../../shared/aiErrors.ts";
import { nativeToolError } from "../features/ai-tools/types.ts";

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

function shortcutEffectSource() {
  let effect;
  function visit(node) {
    if (ts.isCallExpression(node) && node.expression.getText(source) === "useEffect"
      && node.arguments[0]?.getText(source).includes('register("Ctrl+Alt+/"')) effect = node.arguments[0];
    ts.forEachChild(node, visit);
  }
  visit(source);
  assert.ok(effect, "O atalho desktop deve continuar registrado");
  return effect.getText(source);
}

test("Ctrl+Alt+/ alterna o painel em Pressed, ignora Released e dispensa qualquer comando do jogo", async () => {
  let open = false;
  let callback;
  const notices = [];
  const runEffect = compile(shortcutEffectSource(), {
    services: { authScope: { persistent: true } },
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
  const failure = compile(shortcutEffectSource(), {
    services: { authScope: { persistent: true } },
    register: async () => { throw new Error("shortcut unavailable"); },
    setAssistantOpen: () => assert.fail("falha no registro não deve alternar o painel"),
    setNotice: (value) => notices.push(value),
  });
  failure();
  await Promise.resolve();
  assert.match(notices.at(-1), /Ctrl\+Alt\+\/ indisponível/);
});

test("sessão independente não registra nem disputa o atalho global do Assistente", () => {
  const runEffect = compile(shortcutEffectSource(), {
    services: { authScope: { persistent: false } },
    register: () => assert.fail("sessão independente não deve registrar Ctrl+Alt+/"),
    setAssistantOpen: () => assert.fail("o efeito não deve alterar o painel"),
    setNotice: () => assert.fail("não há falha de atalho quando o registro é dispensado"),
  });
  assert.equal(runEffect(), undefined);
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

function panelHarness({ recognitionAvailable = true, ask = async () => "Resposta desktop", instance, requestToolAdvice, native = async () => assert.fail("Nenhum comando nativo esperado") } = {}) {
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
    publicAiError, nativeToolError,
    invoke: native,
    listen: async () => () => {},
    AiToolProposalList: () => null,
    playAuroraSpeech: async (text) => { spoken = text; return { pause: () => { audioPaused += 1; } }; },
  });
  const panelProps = { services: {}, username: "QA", instance, requestToolAdvice, accountId: "synthetic-account", onClose: () => {} };
  const render = () => { slot = 0; effectIndex = 0; return Panel(panelProps); };
  function find(tree, predicate) {
    if (predicate(tree)) return tree;
    return tree.children?.map((child) => typeof child === "object" ? find(child, predicate) : null).find(Boolean);
  }
  return {
    render,
    control: (label) => find(render(), (node) => node.props?.["aria-label"] === label),
    checkbox: () => find(render(), (node) => node.type === "input" && node.props?.type === "checkbox"),
    button: (label) => find(render(), (node) => node.type === "button" && node.children?.includes(label)),
    form: () => find(render(), (node) => node.type === "form"),
    text: () => JSON.stringify(render()),
    recognition: () => recognition,
    spoken: () => spoken,
    audioPaused: () => audioPaused,
    unmount: () => cleanups.forEach((cleanup) => cleanup()),
    changeInstance: (next) => { panelProps.instance = next; render(); },
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
  const panel = panelHarness({ ask: async () => { throw { code: "network" }; } });
  panel.control("Mensagem para o Assistente").props.onChange({ target: { value: "Olá" } });
  await panel.form().props.onSubmit({ preventDefault() {} });
  assert.ok(panel.text().includes(AI_ERRORS.network));
  panel.control("Usar microfone").props.onClick();
  panel.recognition().onerror();
  assert.match(panel.text(), /Não consegui compreender o áudio/);
  const unavailable = panelHarness({ recognitionAvailable: false });
  unavailable.control("Usar microfone").props.onClick();
  assert.match(unavailable.text(), /microfone por voz não está disponível/);
});

test("resposta vazia ou erro do Gemini encerra o carregamento e mostra motivo público", async () => {
  for (const scenario of [
    { ask: async () => "", expected: AI_ERRORS.invalid_response },
    { ask: async () => { throw { code: "not_configured" }; }, expected: AI_ERRORS.not_configured },
    { ask: async () => { throw { code: "model_unavailable" }; }, expected: AI_ERRORS.model_unavailable },
    { ask: async () => { throw new Error("SYNTHETIC_PROVIDER_SECRET"); }, expected: AI_ERRORS.invalid_response },
  ]) {
    const panel = panelHarness(scenario);
    panel.control("Mensagem para o Assistente").props.onChange({ target: { value: "Olá" } });
    await panel.form().props.onSubmit({ preventDefault() {} });
    assert.ok(panel.text().includes(scenario.expected));
    assert.doesNotMatch(panel.text(), /typing-dots|Pensando…|SYNTHETIC_PROVIDER_SECRET/);
    assert.equal(panel.spoken(), undefined);
  }
});

test("lista de mods só segue na consulta marcada e gera sugestões sem executar alterações", async () => {
  const commands = [];
  const context = { schemaVersion: 1, contextId: "synthetic-context", mods: [], settings: [], history: [], truncated: false };
  let calls = 0;
  const panel = panelHarness({
    instance: { id: "synthetic-instance", name: "Instância sintética" },
    native: async (command, args) => {
      commands.push({ command, args });
      if (command === "ai_tools_context") return context;
      if (command === "ai_record_advice") return {};
      assert.fail("Não deve preparar ou aplicar ações automaticamente");
    },
    requestToolAdvice: async (received, question, signal) => {
      calls += 1; assert.equal(received, context); assert.equal(question, "Confira os mods"); assert.equal(signal.aborted, false);
      return { summary: "Sugestão sintética sem alteração.", proposals: [], model: "gemini-test" };
    },
  });
  assert.equal(panel.checkbox().props.checked, false);
  panel.control("Mensagem para o Assistente").props.onChange({ target: { value: "Pergunta comum" } });
  await panel.form().props.onSubmit({ preventDefault() {} });
  assert.equal(calls, 0);
  assert.deepEqual(commands, []);
  panel.checkbox().props.onChange({ target: { checked: true } });
  panel.control("Mensagem para o Assistente").props.onChange({ target: { value: "Confira os mods" } });
  await panel.form().props.onSubmit({ preventDefault() {} });
  assert.equal(calls, 1);
  assert.deepEqual(commands.map((item) => item.command), ["ai_tools_context", "ai_record_advice"]);
  assert.equal(panel.spoken(), "Sugestão sintética sem alteração.");
});

test("cancelar, trocar de instância e fechar descartam respostas tardias do Assistente", async () => {
  for (const finish of ["cancel", "instance", "unmount"]) {
    let resolve;
    let signal;
    const panel = panelHarness({ ask: (_services, _question, receivedSignal) => { signal = receivedSignal; return new Promise((done) => { resolve = done; }); } });
    panel.control("Mensagem para o Assistente").props.onChange({ target: { value: "Olá" } });
    const operation = panel.form().props.onSubmit({ preventDefault() {} });
    if (finish === "cancel") panel.button("Cancelar consulta").props.onClick();
    else if (finish === "instance") panel.changeInstance({ id: "other-instance", name: "Outra instância" });
    else panel.unmount();
    assert.equal(signal.aborted, true);
    resolve("STALE_ASSISTANT_RESPONSE");
    await operation;
    assert.doesNotMatch(panel.text(), /STALE_ASSISTANT_RESPONSE/);
    assert.equal(panel.spoken(), undefined);
  }
});
