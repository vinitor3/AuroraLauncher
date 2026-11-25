import assert from "node:assert/strict";
import test from "node:test";
import {
  actionName,
  combo,
  encoded,
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
    assert.equal(keys.includes("key.keyboard.world.1"), layout === "ABNT2");
  }
});
