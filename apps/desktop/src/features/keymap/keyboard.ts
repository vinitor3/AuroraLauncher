export type Binding = { action: string; input: string; modifier: string };
export type KeyMap = {
  source: "saved" | "defaults" | "empty";
  revision: string | null;
  bindings: Binding[];
  inputBackend?: "glfw" | "sdl" | null;
  modifierSupport:
    | "legacyForge"
    | "modernForge"
    | "brokenForgeReader"
    | "unavailable";
};
export const forgeModifiers = {
  NONE: "Sem modificador",
  ALT: "Alt",
  CONTROL: "Ctrl",
  SHIFT: "Shift",
} as const;
export type ForgeModifier = keyof typeof forgeModifiers;
export function editableModifier(binding: Binding): ForgeModifier | null {
  if (!binding.modifier) return "NONE";
  return Object.hasOwn(forgeModifiers, binding.modifier)
    ? (binding.modifier as ForgeModifier)
    : null;
}
export function modifierLabel(modifier: string) {
  return modifier
    .split(",")
    .filter((key) => key && key !== "NONE")
    .map((key) => forgeModifiers[key as ForgeModifier] ?? key)
    .join(" + ");
}
export function modifierChoices(
  support: KeyMap["modifierSupport"],
  binding: Binding,
) {
  const current = editableModifier(binding);
  if (current === null) return [];
  if (support === "brokenForgeReader" && current !== "NONE")
    return [current, "NONE"] as ForgeModifier[];
  if (
    (support === "legacyForge" && /^-?\d+$/.test(binding.input)) ||
    (support === "modernForge" && !/^-?\d+$/.test(binding.input))
  )
    return Object.keys(forgeModifiers) as ForgeModifier[];
  return [];
}
export function modifierApplies(key: string, modifier: string) {
  if (modifier === "NONE" || !modifier) return true;
  if (key === "key.keyboard.unknown") return false;
  const name = { ALT: "alt", CONTROL: "control", SHIFT: "shift" }[modifier];
  return (
    key !== "key.keyboard.left." + name && key !== "key.keyboard.right." + name
  );
}
export type KeyboardLayout = "ABNT2" | "ANSI";
export type Key = { id: string; label: string; width: number };

const legacy: Record<string, string> = {};
function series(start: number, keys: string) {
  keys.split(" ").forEach((key, offset) => {
    legacy[String(start + offset)] = "key.keyboard." + key;
  });
}
series(
  1,
  "escape 1 2 3 4 5 6 7 8 9 0 minus equal backspace tab q w e r t y u i o p left.bracket right.bracket enter left.control a s d f g h j k l semicolon apostrophe grave.accent left.shift backslash z x c v b n m comma period slash right.shift keypad.multiply left.alt space caps.lock f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 num.lock scroll.lock keypad.7 keypad.8 keypad.9 keypad.subtract keypad.4 keypad.5 keypad.6 keypad.add keypad.1 keypad.2 keypad.3 keypad.0 keypad.decimal",
);
series(87, "f11 f12");
series(100, "f13 f14 f15");
for (const [code, key] of Object.entries({
  83: "keypad.decimal",
  156: "keypad.enter",
  157: "right.control",
  181: "keypad.divide",
  183: "print.screen",
  184: "right.alt",
  197: "pause",
  199: "home",
  200: "up",
  201: "page.up",
  203: "left",
  205: "right",
  207: "end",
  208: "down",
  209: "page.down",
  210: "insert",
  211: "delete",
  219: "left.win",
  220: "right.win",
  221: "menu",
}))
  legacy[code] = "key.keyboard." + key;
for (let n = 0; n < 20; n++)
  legacy[String(n - 100)] =
    "key.mouse." + (["left", "right", "middle"][n] ?? String(n + 1));
legacy["0"] = "key.keyboard.unknown";
Object.assign(legacy, { 83: "NumpadDecimal", 86: "IntlBackslash", 115: "IntlRo", 126: "NumpadComma", 221: "ContextMenu" });
const reverse = new Map(
  Object.entries(legacy).map(([code, key]) => [key, code]),
);
// These positions have different option names in Minecraft's Windows GLFW and SDL clients.
const special: Record<string, Record<"glfw" | "sdl", string>> = {
  IntlBackslash: { glfw: "key.keyboard.world.2", sdl: "key.keyboard.world.1" },
  IntlRo: { glfw: "scancode.115", sdl: "key.keyboard.international1" },
  NumpadComma: { glfw: "scancode.126", sdl: "key.keyboard.keypad.comma" },
  NumpadDecimal: { glfw: "key.keyboard.keypad.decimal", sdl: "key.keyboard.keypad.period" },
  ContextMenu: { glfw: "key.keyboard.menu", sdl: "key.keyboard.application" },
};
const physicalAliases = Object.entries(special);
const domCodes: Record<string, string> = {
  Backquote: "grave.accent", Minus: "minus", Equal: "equal", BracketLeft: "left.bracket",
  BracketRight: "right.bracket", Backslash: "backslash", Semicolon: "semicolon", Quote: "apostrophe",
  Comma: "comma", Period: "period", Slash: "slash", CapsLock: "caps.lock", NumLock: "num.lock",
  ScrollLock: "scroll.lock", PrintScreen: "print.screen", PageUp: "page.up", PageDown: "page.down",
  ShiftLeft: "left.shift", ShiftRight: "right.shift", ControlLeft: "left.control", ControlRight: "right.control",
  AltLeft: "left.alt", AltRight: "right.alt", MetaLeft: "left.win", MetaRight: "right.win",
  ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right",
  NumpadAdd: "keypad.add", NumpadSubtract: "keypad.subtract", NumpadMultiply: "keypad.multiply",
  NumpadDivide: "keypad.divide", NumpadEnter: "keypad.enter", NumpadEqual: "keypad.equal",
};
export function eventKey(code: string): string | undefined {
  if (Object.hasOwn(special, code)) return code;
  if (Object.hasOwn(domCodes, code)) return "key.keyboard." + domCodes[code];
  const simple = /^(?:Key([A-Z])|Digit(\d)|(F(?:[1-9]|1\d|2[0-4]))|(Escape|Backspace|Tab|Enter|Space|Insert|Delete|Home|End|Pause))$/.exec(code);
  if (simple) return "key.keyboard." + (simple[1] ?? simple[2] ?? simple[3] ?? simple[4]).toLowerCase();
  if (/^Numpad\d$/.test(code)) return "key.keyboard.keypad." + code.at(-1);
}
export function eventMouse(button: number): string | undefined {
  return ["key.mouse.left", "key.mouse.middle", "key.mouse.right", "key.mouse.4", "key.mouse.5"][button];
}
export function physical(input: string, backend?: KeyMap["inputBackend"]) {
  if (/^-?\d+$/.test(input)) return legacy[input] ?? input;
  if (backend) {
    for (const [code, names] of physicalAliases) if (names[backend] === input) return code;
  }
  return input;
}
export function encoded(target: string, previous: string, backend?: KeyMap["inputBackend"]) {
  if (/^-?\d+$/.test(previous)) return reverse.get(target);
  if (Object.hasOwn(special, target)) return backend ? special[target][backend] : undefined;
  return target;
}
export function combo(binding: Binding, backend?: KeyMap["inputBackend"]) {
  return (
    binding.modifier
      .split(",")
      .filter((key) => key && key !== "NONE")
      .sort()
      .join("+") +
    "/" +
    physical(binding.input, backend)
  );
}
export function groups(bindings: Binding[], backend?: KeyMap["inputBackend"]) {
  const result = new Map<string, Binding[]>();
  for (const binding of bindings) {
    const id = physical(binding.input, backend);
    if (id === "key.keyboard.unknown") continue;
    const key = combo(binding, backend);
    const list = result.get(key) ?? [];
    list.push(binding);
    result.set(key, list);
  }
  return result;
}
const labels: Record<string, string> = {
  escape: "Esc",
  "grave.accent": "`",
  minus: "−",
  equal: "=",
  backspace: "⌫",
  tab: "Tab",
  "left.bracket": "[",
  "right.bracket": "]",
  backslash: "\\",
  "caps.lock": "Caps",
  semicolon: ";",
  apostrophe: "'",
  enter: "Enter",
  "left.shift": "Shift",
  "right.shift": "Shift",
  comma: ",",
  period: ".",
  slash: "/",
  "left.control": "Ctrl",
  "right.control": "Ctrl",
  "left.win": "Win",
  "right.win": "Win",
  "left.alt": "Alt",
  "right.alt": "Alt",
  space: "Espaço",
  menu: "Menu",
  up: "↑",
  left: "←",
  right: "→",
  down: "↓",
  insert: "Ins",
  delete: "Del",
  "page.up": "PgUp",
  "page.down": "PgDn",
  home: "Home",
  end: "End",
  "num.lock": "Num",
  "print.screen": "PrtSc",
  "scroll.lock": "ScrLk",
  pause: "Pause",
  "world.1": "World 1",
  "world.2": "World 2",
  "keypad.add": "+",
  "keypad.subtract": "−",
  "keypad.multiply": "×",
  "keypad.divide": "÷",
  "keypad.enter": "Enter",
  "keypad.decimal": ".",
};
const abnt: Record<string, string> = {
  "right.alt": "AltGr",
  "grave.accent": "'",
  "left.bracket": "´",
  "right.bracket": "[",
  semicolon: "Ç",
  apostrophe: "~",
  backslash: "]",
  slash: ";",
  "keypad.decimal": ",",
};
export function keyLabel(id: string, layout: KeyboardLayout): string {
  if (id === "IntlBackslash") return "\\";
  if (id === "IntlRo") return "/";
  if (id === "NumpadDecimal") return layout === "ANSI" ? "." : ", / Del";
  if (id === "NumpadComma") return "Num ,";
  if (id === "ContextMenu") return "Menu";
  if (id === "key.keyboard.unknown") return "Sem tecla";
  if (id.startsWith("key.mouse.")) {
    const b = id.slice(10);
    return (
      (
        { left: "Mouse 1", right: "Mouse 2", middle: "Mouse 3" } as Record<
          string,
          string
        >
      )[b] ?? "Mouse " + b
    );
  }
  const k = id.replace(/^key\.keyboard\./, "");
  return (
    (layout === "ABNT2" ? abnt[k] : undefined) ??
    labels[k] ??
    k.replace("keypad.", "").toUpperCase()
  );
}
export function keyDescription(id: string, layout: KeyboardLayout) {
  const name = keyLabel(id, layout);
  if (/^key\.keyboard\.left\./.test(id)) return name + " esquerdo";
  if (/^key\.keyboard\.right\./.test(id)) return name + " direito";
  if (/^key\.keyboard\.keypad\.|^Numpad/.test(id)) return name + " · numérico";
  return name;
}
const widths: Record<string, number> = {
  backspace: 1.8,
  tab: 1.45,
  "caps.lock": 1.65,
  enter: 1.8,
  "left.shift": 2,
  "right.shift": 2,
  "left.control": 1.35,
  "right.control": 1.35,
  space: 5.3,
};
export function rows(layout: KeyboardLayout): Key[][] {
  const codes = [
    "escape f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12",
    "grave.accent 1 2 3 4 5 6 7 8 9 0 minus equal backspace",
    layout === "ABNT2"
      ? "tab q w e r t y u i o p left.bracket right.bracket"
      : "tab q w e r t y u i o p left.bracket right.bracket backslash",
    layout === "ABNT2"
      ? "caps.lock a s d f g h j k l semicolon apostrophe backslash enter"
      : "caps.lock a s d f g h j k l semicolon apostrophe enter",
    layout === "ABNT2"
      ? "left.shift IntlBackslash z x c v b n m comma period slash IntlRo right.shift"
      : "left.shift z x c v b n m comma period slash right.shift",
    "left.control left.win left.alt space right.alt right.win ContextMenu right.control",
  ];
  return codes.map((row) =>
    row.split(" ").map((k) => ({
      id: Object.hasOwn(special, k) ? k : "key.keyboard." + k,
      label: keyLabel(Object.hasOwn(special, k) ? k : "key.keyboard." + k, layout),
      width: widths[k] ?? 1,
    })),
  );
}
export const extras =
  "insert home page.up delete end page.down left up down right print.screen scroll.lock pause num.lock keypad.divide keypad.multiply keypad.subtract keypad.7 keypad.8 keypad.9 keypad.add keypad.4 keypad.5 keypad.6 keypad.1 keypad.2 keypad.3 keypad.enter keypad.0"
    .split(" ")
    .map((k) => "key.keyboard." + k)
    .concat([
      "NumpadDecimal",
      "NumpadComma",
      "key.mouse.left",
      "key.mouse.right",
      "key.mouse.middle",
      "key.mouse.4",
      "key.mouse.5",
    ]);
const vanilla: Record<string, string> = {
  forward: "Avançar",
  back: "Recuar",
  left: "Mover à esquerda",
  right: "Mover à direita",
  jump: "Pular",
  sneak: "Agachar",
  sprint: "Correr",
  attack: "Atacar / destruir",
  use: "Usar item",
  inventory: "Inventário",
  drop: "Soltar item",
  swapOffhand: "Trocar de mão",
  swapHands: "Trocar de mão",
  chat: "Abrir chat",
  playerlist: "Lista de jogadores",
  command: "Comando",
  screenshot: "Captura de tela",
  togglePerspective: "Alternar perspectiva",
  pickItem: "Escolher bloco",
};
export function actionName(action: string) {
  const name = action.replace(/^key\./, "");
  if (Object.prototype.hasOwnProperty.call(vanilla, name)) return vanilla[name];
  if (/^hotbar\.[1-9]$/.test(name)) return "Barra rápida " + name.slice(-1);
  return name.replace(/[.:/_-]+/g, " ").replace(/([a-z])([A-Z])/g, "$1 $2");
}
