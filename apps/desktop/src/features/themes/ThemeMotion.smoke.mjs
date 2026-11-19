// Run from apps/desktop: node src/features/themes/ThemeMotion.smoke.mjs
// Real transpiled components; synthetic DOM/codecs only. No media or credentials are stored.
import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join, resolve, sep } from "node:path";
import test from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";
import ts from "typescript";
import { parseHTML } from "linkedom";
import React, { act } from "react";
import { createRoot } from "react-dom/client";

function gifAsset() {
  const header = Buffer.from([71, 73, 70, 56, 57, 97, 1, 0, 1, 0, 128, 0, 0, 0, 0, 0, 255, 255, 255]);
  const frame = Buffer.from([0x21, 0xf9, 4, 4, 4, 0, 0, 0, 0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x44, 0x01, 0]);
  return { mime: "image/gif", data: Buffer.concat([header, frame, frame, Buffer.from([0x3b])]).toString("base64") };
}

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

test("GIF preparation/playback and cursor cleanup use the actual components", { timeout: 30_000 }, async (t) => {
  const desktop = fileURLToPath(new URL("../../../", import.meta.url));
  const cache = resolve(desktop, "node_modules/.cache");
  await mkdir(cache, { recursive: true });
  const output = await mkdtemp(join(cache, "aurora-motion-smoke-"));
  const saved = new Map();
  const replaceGlobal = (key, value) => {
    if (!saved.has(key)) saved.set(key, Object.getOwnPropertyDescriptor(globalThis, key));
    Object.defineProperty(globalThis, key, { configurable: true, writable: true, value });
  };
  const realTimeout = globalThis.setTimeout;
  const realClearTimeout = globalThis.clearTimeout;
  let root;
  let hidden = false, reduced = false;
  let now = 1000, handle = 0;
  const timers = new Map(), animationFrames = new Map();
  const plans = [], decoders = [], frames = [], earlyDecode = [];
  const contexts = new Map(), windowListeners = new Map(), motionListeners = new Set();
  let supported = true;
  const settle = async (action = () => {}) => {
    await act(async () => { await action(); await new Promise((done) => realTimeout(done, 0)); });
  };
  const advance = async (milliseconds) => {
    now += milliseconds;
    for (const [id, timer] of [...timers]) if (timer.deadline <= now) { timers.delete(id); timer.callback(...timer.args); }
    await settle();
  };
  const runAnimationFrame = async (milliseconds = 16) => {
    now += milliseconds;
    const pending = [...animationFrames.values()]; animationFrames.clear();
    for (const callback of pending) callback(now);
    await settle();
  };

  class MockImageDecoder {
    static async isTypeSupported(mime) { assert.equal(mime, "image/gif"); return supported; }
    constructor(options) {
      this.options = options;
      this.plan = plans.shift() ?? {};
      this.ready = false; this.closed = false; this.closeCalls = 0; this.calls = [];
      this.readyTicket = deferred();
      this.tracks = { ready: this.readyTicket.promise, selectedTrack: { frameCount: this.plan.frameCount ?? 2 } };
      this.pendingDecodes = [];
      decoders.push(this);
      if (!this.plan.manualReady) queueMicrotask(() => this.resolveReady());
    }
    resolveReady() { if (this.closed) return; this.ready = true; this.readyTicket.resolve(); }
    async decode({ frameIndex, completeFramesOnly }) {
      if (!this.ready) { earlyDecode.push(frameIndex); throw new Error("Decode before tracks.ready"); }
      if (this.closed) throw new Error("Decoder is closed");
      assert.equal(completeFramesOnly, true);
      this.calls.push(frameIndex);
      if (this.plan.rejectFrame === frameIndex) throw new Error("Damaged final frame");
      if (this.plan.pendingFrame === frameIndex) {
        const ticket = deferred(); this.pendingDecodes.push(ticket); await ticket.promise;
      }
      const image = {
        displayWidth: 1, displayHeight: 1, duration: 40_000, closeCalls: 0,
        close() { this.closeCalls++; },
      };
      if (this.plan.invalidFrame === frameIndex) image.displayWidth = 0;
      frames.push(image);
      return { image, complete: this.plan.incompleteFrame !== frameIndex };
    }
    close() {
      this.closeCalls++;
      if (this.closed) return;
      this.closed = true;
      if (!this.ready) this.readyTicket.reject(new Error("Decoder closed"));
      if (!this.plan.deliverAfterClose) for (const ticket of this.pendingDecodes) ticket.reject(new Error("Decoder closed"));
    }
  }

  try {
    for (const file of ["model.ts", "gif.ts", "ThemeGif.tsx", "ThemeCursor.tsx"]) {
      const source = (await readFile(new URL(file, import.meta.url), "utf8"))
        .replace(/^import ["']\.\/[^"']+\.css["'];?\r?\n/gm, "")
        .replace(/from ["']\.\/(model|gif)["']/g, (_, name) => `from './${name}.mjs'`);
      const compiled = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, jsx: ts.JsxEmit.ReactJSX } });
      await writeFile(join(output, file.replace(/\.tsx?$/, ".mjs")), compiled.outputText);
    }
    const { window } = parseHTML('<!doctype html><html><body><div id="root"></div></body></html>');
    for (const [key, value] of Object.entries({ window, document: window.document, HTMLElement: window.HTMLElement, Element: window.Element, Event: window.Event, IS_REACT_ACT_ENVIRONMENT: true })) replaceGlobal(key, value);
    Object.defineProperty(document, "hidden", { configurable: true, get: () => hidden });
    window.innerWidth = 1280; window.innerHeight = 800;
    const media = { get matches() { return reduced; }, addEventListener(type, callback) { if (type === "change") motionListeners.add(callback); }, removeEventListener(type, callback) { if (type === "change") motionListeners.delete(callback); } };
    window.matchMedia = () => media;
    const originalAdd = window.addEventListener.bind(window), originalRemove = window.removeEventListener.bind(window);
    window.addEventListener = (type, callback, options) => { if (!windowListeners.has(type)) windowListeners.set(type, new Set()); windowListeners.get(type).add(callback); originalAdd(type, callback, options); };
    window.removeEventListener = (type, callback, options) => { windowListeners.get(type)?.delete(callback); originalRemove(type, callback, options); };
    window.HTMLCanvasElement.prototype.getContext = function () {
      if (!contexts.has(this)) contexts.set(this, { clears: 0, draws: 0, strokes: 0, clearRect() { this.clears++; }, drawImage() { this.draws++; }, beginPath() {}, moveTo() {}, lineTo() {}, stroke() { this.strokes++; } });
      return contexts.get(this);
    };
    replaceGlobal("setTimeout", (callback, delay = 0, ...args) => { const id = ++handle; timers.set(id, { callback, deadline: now + Number(delay), args }); return id; });
    replaceGlobal("clearTimeout", (id) => { if (!timers.delete(id)) realClearTimeout(id); });
    replaceGlobal("requestAnimationFrame", (callback) => { const id = ++handle; animationFrames.set(id, callback); return id; });
    replaceGlobal("cancelAnimationFrame", (id) => { animationFrames.delete(id); });
    replaceGlobal("performance", { now: () => now, measure() {}, getEntriesByType() { return []; } });
    replaceGlobal("ImageDecoder", MockImageDecoder);
    const { openThemeGif, validateThemeGif } = await import(pathToFileURL(join(output, "gif.mjs")));
    const { ThemeGif } = await import(pathToFileURL(join(output, "ThemeGif.mjs")));
    const { ThemeCursor, themeCursorValue } = await import(pathToFileURL(join(output, "ThemeCursor.mjs")));
    const { THEME_EXAMPLES, parseTheme } = await import(pathToFileURL(join(output, "model.mjs")));
    const asset = gifAsset();

    const mount = async (element) => {
      root ??= createRoot(document.getElementById("root"));
      await settle(() => root.render(element));
    };
    const unmount = async () => { if (root) { await settle(() => root.unmount()); root = undefined; } };
    const allClosed = () => {
      assert.ok(decoders.every((decoder) => decoder.closed), "all ImageDecoders must close");
      assert.ok(frames.every((frame) => frame.closeCalls === 1), "every produced VideoFrame must close exactly once");
      assert.equal(timers.size, 0, "no decoder/animation/watchdog timeouts remain");
      assert.equal(animationFrames.size, 0, "no animation-frame request remains");
      assert.equal(earlyDecode.length, 0, "decode never runs before tracks.ready");
    };
    t.afterEach(async () => {
      await unmount();
      for (const decoder of decoders) decoder.close();
      timers.clear(); animationFrames.clear(); plans.length = 0;
      decoders.length = 0; frames.length = 0; earlyDecode.length = 0;
      hidden = false; reduced = false; supported = true; now += 1000;
      replaceGlobal("ImageDecoder", MockImageDecoder);
    });

    await t.test("preloading waits for tracks.ready and closes the first and final decoded frames", async () => {
      plans.push({ manualReady: true });
      let preload;
      await settle(() => { preload = validateThemeGif(asset); });
      assert.equal(decoders.length, 1);
      assert.deepEqual(decoders[0].calls, []);
      assert.equal(decoders[0].closed, false);
      decoders[0].resolveReady();
      await settle(() => preload);
      assert.deepEqual(decoders[0].calls, [0, 1]);
      assert.equal(frames.length, 2);
      allClosed();
    });

    await t.test("damaged final frames and incorrect track counts reject and close resources", async () => {
      for (const plan of [{ invalidFrame: 1 }, { incompleteFrame: 1 }, { rejectFrame: 1 }, { frameCount: 3 }]) {
        plans.push(plan);
        await assert.rejects(validateThemeGif(asset), /quadro inválido|Damaged final|sequência/);
        allClosed();
      }
    });

    await t.test("a stalled track list times out and closes its decoder", async () => {
      plans.push({ manualReady: true });
      let opened;
      await settle(() => { opened = openThemeGif(asset); });
      const rejected = assert.rejects(opened, /demorou.*preparar/);
      await advance(8000);
      await rejected;
      allClosed();
    });

    await t.test("a stalled playback decode trips its watchdog and releases the decoder", async () => {
      const errors = [];
      plans.push({ pendingFrame: 0 });
      await mount(React.createElement(ThemeGif, { asset, paused: false, onError: (message) => errors.push(message) }));
      assert.deepEqual(decoders[0].calls, [0]);
      assert.equal(timers.size, 1);
      await advance(8000);
      assert.equal(errors.length, 1);
      assert.match(errors[0], /GIF/);
      allClosed();
    });

    await t.test("playback pauses, stays idle when hidden, and closes all decoded frames on unmount", async () => {
      const errors = [];
      await mount(React.createElement(ThemeGif, { asset, paused: false, onError: (message) => errors.push(message) }));
      assert.equal(document.querySelector("canvas").dataset.frame, "0");
      assert.equal(timers.size, 1);
      await advance(40);
      assert.equal(document.querySelector("canvas").dataset.frame, "1");
      await mount(React.createElement(ThemeGif, { asset, paused: true }));
      allClosed();
      const count = decoders.length;
      hidden = true;
      await mount(React.createElement(ThemeGif, { asset: { ...asset }, paused: true }));
      assert.equal(decoders.length, count, "a hidden paused GIF never opens a decoder");
      allClosed();
      assert.deepEqual(errors, []);
      await unmount();
      allClosed();
    });

    await t.test("leaving playback while tracks.ready is pending cancels its decoder and timers", async () => {
      for (const action of ["unmount", "pause", "hide"]) {
        hidden = false;
        plans.push({ manualReady: true });
        await mount(React.createElement(ThemeGif, { asset, paused: false }));
        const opening = decoders.at(-1);
        assert.equal(opening.closed, false);
        if (action === "unmount") await unmount();
        else {
          hidden = action === "hide";
          await mount(React.createElement(ThemeGif, { asset, paused: true }));
        }
        assert.equal(opening.closed, true, `pending preparation must close immediately on ${action}`);
        await unmount();
        allClosed();
      }
    });

    await t.test("a frame delivered after unmount is closed and cannot schedule another animation", async () => {
      plans.push({ pendingFrame: 0, deliverAfterClose: true });
      await mount(React.createElement(ThemeGif, { asset, paused: false }));
      assert.deepEqual(decoders[0].calls, [0]);
      await unmount();
      for (const ticket of decoders[0].pendingDecodes) ticket.resolve();
      await settle();
      assert.equal(frames.length, 1);
      allClosed();
    });

    await t.test("unsupported browsers receive a useful GIF fallback message", async () => {
      supported = false;
      await assert.rejects(openThemeGif(asset), /WebView2.*MP4\/WebM/);
      replaceGlobal("ImageDecoder", undefined);
      await assert.rejects(openThemeGif(asset), /WebView2.*MP4\/WebM/);
      assert.equal(decoders.length, 0);
      allClosed();
    });

    await t.test("cursor URIs contain only trusted inline artwork and validated presentation values", () => {
      assert.equal(themeCursorValue(THEME_EXAMPLES[0].cursor), "auto");
      for (const style of ["pixel", "ring", "crosshair"]) for (const size of [16, 40]) {
        const theme = parseTheme(JSON.stringify({ ...THEME_EXAMPLES[0], cursor: { style, color: "#abcdef", size, trail: "none" } }));
        const value = themeCursorValue(theme.cursor);
        const match = /^url\("data:image\/svg\+xml,([^"\s]+)"\) (\d+) (\d+), auto$/.exec(value);
        assert.ok(match, "cursor remains an internal encoded data URI with bounded hotspots");
        const svg = decodeURIComponent(match[1]);
        assert.ok(svg.includes(`width="${size}"`));
        assert.ok(svg.includes("#abcdef"));
        assert.doesNotMatch(svg, /<script|onload|href=|javascript:|<foreignObject/);
        assert.ok(Number(match[2]) < size && Number(match[3]) < size);
      }
      assert.throws(() => parseTheme(JSON.stringify({ ...THEME_EXAMPLES[0], cursor: { style: "ring", color: '\" onload="alert(1)', size: 24, trail: "none" } })), /#RRGGBB/);
    });

    await t.test("cursor trails stop for reduced motion, hidden windows, protected areas and unmount", async () => {
      const theme = { ...THEME_EXAMPLES[0], cursor: { style: "ring", color: "#abcdef", size: 24, trail: "sparkle" } };
      await mount(React.createElement("div", { className: "launcher-theme-root" }, React.createElement("button", { id: "content-target" }, "Conteúdo"), React.createElement("div", { className: "theme-manager" }, React.createElement("button", { id: "protected-target" }, "Editor")), React.createElement("section", { className: "theme-protected" }, React.createElement("button", { id: "consent-target" }, "Confirmar entrada")), React.createElement(ThemeCursor, { theme })));
      const pointer = (target = document.getElementById("content-target"), pointerType = "mouse") => {
        now += 50;
        for (const callback of windowListeners.get("pointermove") ?? []) callback({ target, pointerType, clientX: 50, clientY: 60 });
      };
      pointer();
      assert.equal(animationFrames.size, 1);
      await runAnimationFrame();
      assert.ok([...contexts.values()].some((context) => context.strokes > 0));
      reduced = true;
      for (const callback of motionListeners) callback();
      assert.equal(animationFrames.size, 0);
      pointer(); assert.equal(animationFrames.size, 0);
      reduced = false; hidden = true;
      document.dispatchEvent(new window.Event("visibilitychange"));
      pointer(); assert.equal(animationFrames.size, 0);
      hidden = false;
      pointer(document.getElementById("protected-target")); assert.equal(animationFrames.size, 0);
      pointer(); assert.equal(animationFrames.size, 1);
      pointer(document.getElementById("consent-target")); assert.equal(animationFrames.size, 0, "entering a security review clears the decorative trail");
      pointer(document.body); assert.equal(animationFrames.size, 0);
      pointer(document.getElementById("content-target"), "touch"); assert.equal(animationFrames.size, 0);
      pointer(); assert.equal(animationFrames.size, 1);
      await unmount();
      allClosed();
      assert.equal(motionListeners.size, 0);
      assert.equal(windowListeners.get("pointermove")?.size ?? 0, 0);
      assert.equal(windowListeners.get("blur")?.size ?? 0, 0);
      const clears = [...contexts.values()].reduce((sum, context) => sum + context.clears, 0);
      document.dispatchEvent(new window.Event("visibilitychange"));
      assert.equal([...contexts.values()].reduce((sum, context) => sum + context.clears, 0), clears, "the visibility listener is removed on unmount");
    });
  } finally {
    if (root) await settle(() => root.unmount());
    for (const decoder of decoders) decoder.close();
    timers.clear(); animationFrames.clear();
    for (const [key, descriptor] of saved) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key];
    }
    if (!resolve(output).startsWith(`${cache}${sep}`)) throw new Error("Unexpected motion-test cache path.");
    await rm(output, { recursive: true, force: true });
  }
});
