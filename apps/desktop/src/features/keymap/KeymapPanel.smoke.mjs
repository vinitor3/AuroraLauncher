import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
import ts from "typescript";
const require = createRequire(import.meta.url);
const {window,document} = require("linkedom").parseHTML("<html><body><div id='root'></div></body></html>");
Object.assign(globalThis,{window,document,HTMLElement:window.HTMLElement,IS_REACT_ACT_ENVIRONMENT:true});
Object.defineProperty(globalThis,"navigator",{configurable:true,value:{userAgent:"Keymap DOM"}});
window.HTMLElement.prototype.scrollIntoView = () => {};
const { act, createElement } = await import("react");
const {createRoot} = await import("react-dom/client");
const moduleUrl = js => "data:text/javascript;base64,"+Buffer.from(js).toString("base64");
async function compile(file, dependencies) {
  const code = ts.transpileModule(await readFile(new URL(file,import.meta.url),"utf8"),{fileName:file,compilerOptions:{
    module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX,
  }}).outputText.replace(/^import ["'][^"']+\.css["'];?$/gm,"");
  return moduleUrl(code.replace(/from "([^"]+)"/g,(_,spec) => "from "+JSON.stringify(dependencies[spec] ?? pathToFileURL(require.resolve(spec)).href)));
}
const keyboard = await compile("./keyboard.ts",{});
const native = moduleUrl("export const invoke=(...args)=>globalThis.keymapInvoke(...args)");
const {default:Panel} = await import(await compile("./KeymapPanel.tsx",{"./keyboard":keyboard,"@tauri-apps/api/core":native}));

test("choose only Alt, save, inspect distinct shortcuts and remove it on the original C key", async () => {
  const data = {source:"saved",revision:"initial",modifierSupport:"modernForge",bindings:[
    {action:"key.one",input:"key.keyboard.c",modifier:""},{action:"key.two",input:"key.keyboard.c",modifier:""},
  ]};
  const calls = [];
  globalThis.keymapInvoke = async (command,args) => {
    if (command === "rebind_instance_key") {
      calls.push(args);
      const row = data.bindings.find(b => b.action === args.action);
      row.modifier = args.modifier === "NONE" ? "" : args.modifier; data.revision += ".";
    }
    return structuredClone(data);
  };
  const box = document.getElementById("root"), root = createRoot(box);
  const click = async text => { const b = [...box.querySelectorAll("button")].find(b => b.textContent===text);
    assert.ok(b); assert.equal(b.disabled,false); await act(async () => b.dispatchEvent(new window.Event("click",{bubbles:true}))); };
  const set = async value => { const el = box.querySelector('select[aria-label="Modificador"]'); assert.ok(el);
    assert.ok([...el.querySelectorAll("option")].some(opt => opt.value===value));
    Object.defineProperty(el,"value",{configurable:true,value});
    await act(async () => el.dispatchEvent(new window.Event("change",{bubbles:true}))); };
  try {
    await act(async () => root.render(createElement(Panel,{instanceId:"qa",running:false})));
    await click("Alterar"); await set("ALT");
    assert.match(box.querySelector(".km-edit").textContent,/contexto/);
    await click("Salvar tecla");
    assert.deepEqual([calls[0].input,calls[0].modifier],["key.keyboard.c","ALT"]);
    assert.equal(box.querySelectorAll(".km-action .km-warning").length,0);
    assert.match(box.querySelector(".km-action kbd").textContent,/Alt \+ C/);
    await click("Alterar"); await set("NONE"); await click("Salvar tecla");
    assert.equal(calls[1].modifier,"NONE"); assert.equal(box.querySelectorAll(".km-action .km-warning").length,2);
    data.modifierSupport="unavailable"; await click("Recarregar"); await click("Alterar");
    assert.equal(box.querySelector('select[aria-label="Modificador"]'),null);
  } finally { await act(async () => root.unmount()); delete globalThis.keymapInvoke; }
});
