import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";
import { drawSkinPortrait } from "../src/lib/skinFace.ts";

// Exercise effects and image callbacks without requiring a GPU or a new DOM dependency.
function setup({ constructorFails = false, textureFails = false, thumbnail = false } = {}) {
  const slots = [], effects = [], images = [], viewers = [], timers = new Map();
  const document = Object.assign(new EventTarget(), { hidden: false });
  const drawing = { canvas: {}, clearRect() {}, drawImage() {} };
  let cursor = 0, timerId = 0;
  const hooks = {
    useRef(value) {
      const slot = cursor++;
      return slots[slot] ??= { current: slot === 0 ? { getContext: () => drawing } : value };
    },
    useState(value) {
      const slot = cursor++;
      if (!(slot in slots)) slots[slot] = value;
      return [slots[slot], (next) => { slots[slot] = typeof next === "function" ? next(slots[slot]) : next; }];
    },
    useEffect(effect, dependencies) {
      const slot = cursor++;
      const previous = slots[slot];
      if (previous && dependencies.every((value, index) => Object.is(value, previous.dependencies[index]))) return;
      effects.push(() => {
        previous?.cleanup?.();
        slots[slot] = { dependencies, cleanup: effect() };
      });
    },
  };
  class Image {
    naturalWidth = 64;
    naturalHeight = 64;
    constructor() { images.push(this); }
  }
  class SkinViewer {
    controls = new EventTarget();
    loaded = [];
    visible = false;
    constructor() {
      if (constructorFails) throw new Error("No WebGL");
      viewers.push(this);
    }
    resetSkin() { this.visible = false; }
    render() { this.rendered = true; }
    loadSkin(image, options) {
      if (textureFails) throw new Error("Invalid texture");
      this.loaded.push({ image, options });
      this.visible = true;
    }
    dispose() { this.disposed = true; }
  }
  const runtime = { jsx: (type, props, key) => ({ type, props, key }), jsxs: (type, props, key) => ({ type, props, key }) };
  const exports = {};
  const filename = thumbnail ? "SkinThumbnail" : "SkinPreviewCanvas";
  const code = ts.transpileModule(readFileSync(new URL(`../src/features/skins/${filename}.tsx`, import.meta.url), "utf8"), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  vm.runInNewContext(code, {
    exports, Image, document,
    window: { setTimeout: (fn) => { timers.set(++timerId, fn); return timerId; }, clearTimeout: (id) => timers.delete(id) },
    require: (name) => name === "react" ? hooks : name === "react/jsx-runtime" ? runtime
      : name === "skinview3d" ? { SkinViewer, WalkingAnimation: class {} } : { drawSkinPortrait, SkinThumbnail: "SkinThumbnail" },
  });
  const render = (props, flush = true) => {
    cursor = 0;
    const element = exports[filename](props);
    const result = thumbnail ? element.type(element.props) : element;
    if (flush) while (effects.length) effects.shift()();
    return result;
  };
  return { render, images, viewers, timers, document, unmount: () => slots.forEach((slot) => slot?.cleanup?.()) };
}

const first = { skinUrl: "blob:first", skinModel: "classic" };
const second = { skinUrl: "blob:second", skinModel: "slim" };
const canvas = (tree) => tree.props.children[0].props.children[0];
const placeholder = (tree) => tree.props.children[0].props.children[1];

test("preview renders on demand until motion is requested and pauses in a hidden window", () => {
  const run = setup();
  run.render(first);
  run.images[0].onload();
  const tree = run.render(first);
  const viewer = run.viewers[0];
  assert.equal(viewer.renderPaused, true);
  assert.equal(viewer.rendered, true);
  tree.props.children[1].props.children[1].props.onClick();
  run.render(first);
  assert.ok(viewer.animation);
  assert.equal(viewer.renderPaused, false);
  run.document.hidden = true;
  run.document.dispatchEvent(new Event("visibilitychange"));
  assert.equal(viewer.renderPaused, true);
  run.document.hidden = false;
  run.document.dispatchEvent(new Event("visibilitychange"));
  assert.equal(viewer.renderPaused, false);
  run.unmount();
});

test("3D preview stays hidden until a valid skin loads and hides immediately on selection change", () => {
  const run = setup();
  assert.equal(canvas(run.render(first)).props.style.visibility, "hidden");
  run.images[0].onload();
  assert.equal(canvas(run.render(first)).props.style.visibility, "visible");
  assert.equal(canvas(run.render(second, false)).props.style.visibility, "hidden");
  run.render(second);
  run.images.at(-1).onload();
  assert.equal(canvas(run.render(second)).props.style.visibility, "visible");
  run.unmount();
});

test("late image events and events after unmount cannot replace the selected texture", () => {
  const run = setup();
  run.render(first);
  const lateLoad = run.images[0].onload, lateError = run.images[0].onerror;
  run.render(second);
  lateLoad();
  lateError();
  assert.equal(run.viewers[0].loaded.length, 0);
  run.images[1].onload();
  assert.equal(run.viewers[0].loaded.length, 1);
  const afterUnmount = run.images[1].onload;
  run.unmount();
  afterUnmount();
  assert.equal(run.viewers[0].loaded.length, 1);
  assert.equal(run.viewers[0].disposed, true);
  assert.equal(run.timers.size, 0);
});

test("invalid dimensions, image errors and timeout show an explicit error, never a black mannequin", () => {
  for (const failure of ["dimensions", "error", "timeout"]) {
    const run = setup();
    run.render(first);
    if (failure === "dimensions") { run.images[0].naturalWidth = 128; run.images[0].onload(); }
    else if (failure === "error") run.images[0].onerror();
    else [...run.timers.values()][0]();
    const result = run.render(first);
    assert.equal(canvas(result).props.style.visibility, "hidden");
    assert.match(placeholder(result).props.className, /error/);
    assert.equal(run.viewers[0].loaded.length, 0);
    run.images[0].onload();
    assert.equal(run.viewers[0].loaded.length, 0);
    run.unmount();
  }
});

test("valid legacy 64x32 skins are supported and a cleared selection resets the texture", () => {
  const run = setup();
  run.render(first);
  run.images[0].naturalHeight = 32;
  run.images[0].onload();
  assert.equal(canvas(run.render(first)).props.style.visibility, "visible");
  const empty = run.render({ ...first, skinUrl: "" });
  assert.equal(canvas(empty).props.style.visibility, "hidden");
  assert.equal(run.viewers[0].visible, false);
  run.unmount();
});

test("WebGL initialization and texture decoding failures stay inside the preview", () => {
  for (const options of [{ constructorFails: true }, { textureFails: true }]) {
    const run = setup(options);
    run.render(first);
    run.images[0]?.onload();
    const fallback = placeholder(run.render(first));
    if (options.constructorFails) {
      assert.equal(fallback.type, "SkinThumbnail");
      assert.equal(fallback.props.src, first.skinUrl);
    } else assert.match(fallback.props.className, /error/);
    run.unmount();
  }
});

test("thumbnail hides failed/invalid images and shows a named fallback", () => {
  const run = setup({ thumbnail: true });
  const props = { src: "blob:skin", name: "Link" };
  let tree = run.render(props);
  const image = tree.props.children[1];
  assert.equal(tree.props.children[0].props.style.visibility, "hidden");
  image.props.onLoad({ currentTarget: { naturalWidth: 64, naturalHeight: 64 } });
  assert.equal(run.render(props).props.children[0].props.style.visibility, "visible");
  image.props.onError();
  tree = run.render(props);
  assert.equal(tree.props.children[0].props.style.visibility, "hidden");
  assert.match(tree.props.children[2].props["aria-label"], /Link/);
  image.props.onLoad({ currentTarget: { naturalWidth: 32, naturalHeight: 32 } });
  assert.equal(run.render(props).props.children[0].props.style.visibility, "hidden");
});
