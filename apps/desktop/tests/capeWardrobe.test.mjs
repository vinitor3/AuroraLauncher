import test from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { readFileSync } from "node:fs";
import ts from "typescript";

const source = readFileSync(new URL("../src/features/skins/CapeWardrobe.tsx", import.meta.url), "utf8");
const js = ts.transpileModule(source, { compilerOptions: {
  target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX,
} }).outputText;
const png = new File([Uint8Array.from([137, 80, 78, 71, 1])], "qa.png", { type: "image/png" });
const data = "data:image/png;base64,iVBORwE=";

function cloud() {
  return { profiles: new Map(), images: new Map(), uploads: 0, offline: false };
}

function session(shared, initialOwner) {
  const disk = new Map(), browser = new Map(), slots = [], effects = [];
  const services = { auth: { currentUser: { uid: initialOwner } } };
  let index = 0, scheduled = false, state;
  let profile = { uid: initialOwner, capeUrl: shared.profiles.get(initialOwner) ?? null };
  function schedule() {
    if (!scheduled) { scheduled = true; queueMicrotask(() => { scheduled = false; render(); }); }
  }
  const hooks = {
    useState(initial) {
      const i = index++;
      if (!(i in slots)) slots[i] = initial;
      return [slots[i], next => {
        const value = typeof next === "function" ? next(slots[i]) : next;
        if (!Object.is(value, slots[i])) { slots[i] = value; schedule(); }
      }];
    },
    useRef(initial) { const i = index++; return slots[i] ??= { current: initial }; },
    useEffect(fn, deps) {
      const i = index++, old = slots[i];
      if (!old || !deps.every((value, j) => Object.is(value, old.deps[j]))) {
        slots[i] = { deps }; effects.push(() => { old?.dispose?.(); slots[i].dispose = fn(); });
      }
    },
  };
  async function invoke(name, args) {
    const path = `E:/appearance/${args.userId}/cape.png`;
    if (name === "save_local_appearance") { disk.set(path, args.dataBase64); return path; }
    if (name === "load_local_appearance" && disk.has(path)) return { localPath: path, dataBase64: disk.get(path) };
    if (name === "load_appearance_url" && shared.images.has(args.url)) return { dataBase64: shared.images.get(args.url) };
    throw new Error("unavailable");
  }
  const firebase = {
    async loadAuroraCape(_, uid) { return shared.profiles.get(uid) ?? null; },
    async saveAuroraCape(_, uid, url) { if (shared.offline) throw new Error(); shared.profiles.set(uid, url); },
    async uploadFirebaseAppearanceImage() { throw new Error("offline"); },
  };
  const edge = { async uploadAuroraAppearanceImage(_, kind, file) {
    assert.equal(kind, "cape"); assert.equal(file.type, "image/png");
    if (shared.beforeUpload) await shared.beforeUpload;
    if (shared.offline) throw new Error("offline");
    const url = `https://test.invalid/${++shared.uploads}.png`;
    shared.images.set(url, data);
    return url;
  } };
  const modules = { react: hooks, "react/jsx-runtime": {}, "@tauri-apps/api/core": { invoke },
    "../../lib/firebase": firebase, "../../lib/edge": edge };
  const scope = { exports: {}, require: name => modules[name], fetch, File, Blob,
    FileReader: class { readAsDataURL() { this.result = data; this.onload(); } },
    createImageBitmap: async () => ({ width: 64, height: 32, close() {} }),
    localStorage: { getItem: key => browser.get(key) ?? null, setItem: (key, value) => browser.set(key, value) },
  };
  vm.runInNewContext(js, scope);
  function render() {
    index = 0;
    state = scope.exports.useWardrobeCape(services, profile, (uid, capeUrl) => {
      if (uid === profile.uid) { profile = { ...profile, capeUrl }; schedule(); }
    });
    while (effects.length) effects.shift()();
  }
  render();
  return {
    get state() { return state; },
    disk,
    switch(uid) {
      services.auth.currentUser = { uid };
      profile = { uid, capeUrl: shared.profiles.get(uid) ?? null }; render();
    },
    async wait(check) {
      for (let n = 0; n < 150 && !check(state); n++) await new Promise(resolve => setTimeout(resolve, 2));
      assert.ok(check(state), "wardrobe did not reach the expected state");
    },
  };
}

test("applies independently of body skin, restores from a second installation, and clears the first on refresh", async () => {
  const shared = cloud(), first = session(shared, "alice");
  await first.wait(s => s.ready);
  assert.equal(first.state.share, false);
  await first.state.choose(png); await first.wait(s => !!s.selected);
  assert.equal(first.state.active.path, "");
  first.state.setShare(true);
  await first.state.equip(); await first.wait(s => s.ready && !!s.active.url && !s.active.pending);
  const second = session(shared, "alice");
  await second.wait(s => s.ready && !!s.active.path);
  assert.equal(second.state.share, false);
  assert.equal(second.state.active.png, first.state.active.png);
  await second.state.equip(true); await second.wait(s => s.ready && !s.active.path && !s.active.pending);
  await first.state.reload(); await first.wait(s => s.ready && !s.active.path);
  assert.equal(shared.profiles.get("alice"), null);
  assert.equal(shared.uploads, 1);
});

test("keeps a cape usable offline, retries it, and fences completion after an account switch", async () => {
  const shared = cloud(), player = session(shared, "alice");
  shared.offline = true;
  await player.wait(s => s.ready);
  await player.state.choose(png); await player.wait(s => !!s.selected);
  await player.state.equip(); await player.wait(s => s.active?.pending && !s.working);
  assert.equal(player.state.active.png, data);
  assert.equal(shared.profiles.has("alice"), false);
  shared.offline = false;
  let finish;
  shared.beforeUpload = new Promise(resolve => { finish = resolve; });
  const sending = player.state.equip();
  await player.wait(s => s.working);
  player.switch("bob"); await player.wait(s => s.ready && s.active.owner === "bob");
  finish(); await sending;
  assert.equal(player.state.active.path, "");
  assert.equal(shared.profiles.size, 0);
  player.switch("alice"); await player.wait(s => s.ready && s.active.pending);
  await player.state.equip(); await player.wait(s => s.ready && !!s.active.url);
  assert.ok(shared.profiles.get("alice"));
});
