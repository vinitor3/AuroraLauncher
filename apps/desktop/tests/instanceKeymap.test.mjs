import assert from "node:assert/strict";
import test from "node:test";
import {
  actionName,
  combo,
  encoded,
  eventKey,
  eventMouse,
  groups,
  keyLabel,
  physical,
  rows,
} from "../src/features/keymap/keyboard.ts";

const bind = (action, input, modifier = "") => ({ action, input, modifier });

test("compara apenas comandos da instância e respeita modificadores e teclas livres", () => {
  const defaults = [
    bind("map.open", "key.keyboard.r"),
    bind("radar", "19", "NONE"),
    bind("zoom", "19", "SHIFT"),
    bind("other", "0"),
    bind("free", "key.keyboard.unknown"),
  ];
  const combos = groups(defaults);
  assert.equal(combos.size, 2);
  assert.deepEqual(
    combos.get("/key.keyboard.r").map((b) => b.action),
    ["map.open", "radar"],
  );
  assert.equal(
    combo(bind("a", "19", "SHIFT,CONTROL")),
    combo(bind("b", "key.keyboard.r", "CONTROL,SHIFT")),
  );
  assert.equal(
    groups([bind("otherPack", "19")]).get("/key.keyboard.r").length,
    1,
  );
});

test("mantém códigos legados, mouse, layouts e nomes de mod desconhecidos", () => {
  assert.equal(encoded("key.keyboard.space", "19"), "57");
  assert.equal(encoded("key.mouse.right", "57"), "-99");
  assert.equal(physical("-100"), "key.mouse.left");
  assert.equal(
    encoded("key.keyboard.space", "key.keyboard.a"),
    "key.keyboard.space",
  );
  assert.equal(encoded("scancode.999", "19"), undefined);
  assert.equal(keyLabel("key.keyboard.semicolon", "ABNT2"), "Ç");
  assert.equal(keyLabel("key.keyboard.semicolon", "ANSI"), ";");
  assert.equal(actionName("key.jump"), "Pular");
  assert.equal(actionName("constructor"), "constructor");
  for (const layout of ["ANSI", "ABNT2"]) {
    const keys = rows(layout)
      .flat()
      .map((k) => k.id);
    assert.equal(keys.length, new Set(keys).size);
    assert.equal(keys.includes("IntlBackslash"), layout === "ABNT2");
    assert.equal(keys.includes("IntlRo"), layout === "ABNT2");
  }
});

test("as posições físicas ABNT2, o numérico e o menu persistem no dialeto do cliente Windows", () => {
  for (const [code, legacy, glfw, sdl] of [
    ["IntlBackslash", "86", "key.keyboard.world.2", "key.keyboard.world.1"],
    ["IntlRo", "115", "scancode.115", "key.keyboard.international1"],
    ["NumpadComma", "126", "scancode.126", "key.keyboard.keypad.comma"],
    ["NumpadDecimal", "83", "key.keyboard.keypad.decimal", "key.keyboard.keypad.period"],
    ["ContextMenu", "221", "key.keyboard.menu", "key.keyboard.application"],
  ]) {
    assert.equal(eventKey(code), code);
    assert.equal(encoded(code, "19"), legacy);
    assert.equal(physical(legacy), code);
    assert.equal(encoded(code, "key.keyboard.a", null), undefined);
    for (const [backend, input] of [["glfw", glfw], ["sdl", sdl]]) {
      assert.equal(encoded(code, "key.keyboard.a", backend), input);
      assert.equal(physical(input, backend), code);
      assert.equal(combo(bind("one", input), backend), combo(bind("two", legacy), backend));
    }
  }
});

test("o código físico separa lados, numérico, botões do mouse e teclas de texto", () => {
  for (const [code, id] of [["KeyA","a"],["Digit1","1"],["Numpad1","keypad.1"],["ShiftLeft","left.shift"],
    ["AltRight","right.alt"],["ControlLeft","left.control"],["ArrowLeft","left"],["Semicolon","semicolon"],["F24","f24"]]) {
    assert.equal(eventKey(code), "key.keyboard." + id);
  }
  for (const code of ["Unidentified","AudioVolumeUp","Key", "F99","constructor"]) assert.equal(eventKey(code), undefined);
  assert.deepEqual([0,1,2,3,4,5].map(eventMouse), ["key.mouse.left","key.mouse.middle","key.mouse.right","key.mouse.4","key.mouse.5",undefined]);
});
