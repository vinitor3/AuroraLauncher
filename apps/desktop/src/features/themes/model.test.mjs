import assert from "node:assert/strict";
import test from "node:test";
import { DEFAULT_THEME, THEME_EXAMPLES, THEME_LIMITS, THEME_WINDOW_DEFAULTS, THEME_CURSOR_DEFAULTS, inspectGif, parseTheme, themeVariables, formatThemeCode } from "./model.ts";

const fixture = () => structuredClone(THEME_EXAMPLES[0]);
function read(change) { const theme = fixture(); change?.(theme); return parseTheme(JSON.stringify(theme)); }
const png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a6f8AAAAASUVORK5CYII=";

// A real one-pixel LZW stream, reused in bounded image rectangles. No media fixture is stored.
function gifFixture({ width = 1, height = 1, frames = 1, delays = Array(frames).fill(0), signature = "GIF89a", loop = true, localPalette = false } = {}) {
  const header = Buffer.alloc(13);
  header.write(signature); header.writeUInt16LE(width, 6); header.writeUInt16LE(height, 8); header[10] = localPalette ? 0 : 0x80;
  const palette = Buffer.from([0, 0, 0, 255, 255, 255]);
  const parts = [header];
  if (!localPalette) parts.push(palette);
  if (loop) parts.push(Buffer.from([0x21, 0xff, 11]), Buffer.from("NETSCAPE2.0"), Buffer.from([3, 1, 0, 0, 0]));
  for (let i = 0; i < frames; i++) {
    if (delays[i] !== undefined) parts.push(Buffer.from([0x21, 0xf9, 4, 4, delays[i] & 255, delays[i] >> 8, 0, 0]));
    parts.push(Buffer.from([0x2c, 0, 0, 0, 0, 1, 0, 1, 0, localPalette ? 0x80 : 0]));
    if (localPalette) parts.push(palette);
    parts.push(Buffer.from([2, 2, 0x44, 0x01, 0]));
  }
  parts.push(Buffer.from([0x3b]));
  return Buffer.concat(parts);
}

test("os exemplos passam pelo schema fechado e o padrão tem id reservado", () => {
  for (const example of THEME_EXAMPLES) assert.deepEqual(parseTheme(JSON.stringify(example)), example);
  assert.throws(() => parseTheme(JSON.stringify(DEFAULT_THEME)), /reservado/);
});

test("documentos v1 antigos continuam válidos e recebem os novos padrões completos", () => {
  const old = fixture();
  old.layout.navOrder = ["java", "appearance", "instances", "discover"];
  delete old.icons.connect;
  delete old.icons.social;
  delete old.window;
  delete old.cursor;
  for (const key of ["sidebarWidth", "statusSide", "sectionOrder", "decorationHeight"]) delete old.layout[key];
  for (const key of ["transition", "transitionMs"]) delete old.effects[key];
  old.decorations = [{ type: "text", text: "Tema antigo", asset: null, align: "left" }];
  const parsed = parseTheme(JSON.stringify(old));
  assert.deepEqual(parsed.layout.navOrder, [...old.layout.navOrder, "connect", "social"]);
  assert.deepEqual(parsed.icons, { ...old.icons, connect: "default", social: "default" });
  assert.deepEqual(parsed.window, THEME_WINDOW_DEFAULTS);
  assert.deepEqual(parsed.cursor, THEME_CURSOR_DEFAULTS);
  assert.equal(parsed.layout.sidebarWidth, 240);
  assert.equal(parsed.layout.statusSide, "right");
  assert.deepEqual(parsed.layout.sectionOrder, ["header", "decorations", "content"]);
  assert.equal(parsed.layout.decorationHeight, 160);
  assert.equal(parsed.effects.transition, "fade");
  assert.equal(parsed.effects.transitionMs, 300);
  assert.deepEqual(parsed.decorations[0], { ...old.decorations[0], x: 50, y: 50, width: 30, height: 64, fontSize: 20 });
  const partial = read((x) => { x.window = { titlebarHeight: 56 }; });
  assert.deepEqual(partial.window, { ...THEME_WINDOW_DEFAULTS, titlebarHeight: 56 });
  assert.throws(() => { delete old.typography.font; parseTheme(JSON.stringify(old)); }, /obrigatório/);
});

test("navegação atual preserva as seis seções e recusa formatos legados misturados", () => {
  const current = read((x) => {
    x.layout.navOrder = ["social", "java", "connect", "appearance", "instances", "discover"];
    x.icons.connect = "gamepad";
    x.icons.social = "heart";
  });
  assert.deepEqual(parseTheme(JSON.stringify(current)), current, "normalização permanece idempotente");
  for (const change of [
    (x) => { delete x.icons.connect; },
    (x) => { delete x.icons.connect; delete x.icons.social; },
    (x) => { x.layout.navOrder = ["instances", "discover", "appearance", "java"]; },
    (x) => { x.layout.navOrder.pop(); },
    (x) => { x.layout.navOrder[5] = "connect"; },
    (x) => { x.layout.navOrder[5] = "shell"; },
    (x) => { x.icons.social = null; },
    (x) => { x.icons.social = "asset:missing"; },
    (x) => { x.icons.shell = "default"; },
  ]) assert.throws(() => read(change));
  assert.match(formatThemeCode(current), /connect e social/);
});

test("cursor aceita somente estilos internos, cores sólidas, tamanhos e rastros limitados", () => {
  const partial = read((x) => { x.cursor = { style: "pixel" }; });
  assert.deepEqual(partial.cursor, { ...THEME_CURSOR_DEFAULTS, style: "pixel" });
  for (const style of ["system", "pixel", "ring", "crosshair"]) assert.equal(read((x) => { x.cursor.style = style; }).cursor.style, style);
  for (const trail of ["none", "sparkle"]) assert.equal(read((x) => { x.cursor.trail = trail; }).cursor.trail, trail);
  for (const size of [16, 40]) assert.equal(read((x) => { x.cursor.size = size; }).cursor.size, size);
  for (const mutate of [
    (x) => { x.cursor = null; }, (x) => { x.cursor.color = "url(https://evil.example/cursor.svg)"; },
    (x) => { x.cursor.style = "javascript:alert(1)"; }, (x) => { x.cursor.trail = "script"; },
    (x) => { x.cursor.size = 15; }, (x) => { x.cursor.size = 41; }, (x) => { x.cursor.size = null; },
    (x) => { x.cursor.onMove = "invoke('shell')"; },
  ]) assert.throws(() => read(mutate));
});

test("GIF87a/89a têm estrutura completa, palettes, frames e atrasos efetivos limitados", () => {
  assert.deepEqual(inspectGif(gifFixture()), { width: 1, height: 1, frames: 1, durationMs: 40 });
  assert.deepEqual(inspectGif(gifFixture({ signature: "GIF87a", localPalette: true, loop: false, delays: [] })), { width: 1, height: 1, frames: 1, durationMs: 40 });
  assert.equal(inspectGif(gifFixture({ frames: 3, delays: [0, 5, 10] })).durationMs, 190);
  assert.equal(inspectGif(gifFixture({ frames: 2, delays: [100] })).durationMs, 1040, "GCE vale para o próximo frame, depois volta ao mínimo");
  const gif = gifFixture();
  const comment = Buffer.concat([Buffer.from([0x21, 0xfe, 14]), Buffer.from("https://no.run"), Buffer.from([0])]);
  comment[2] = Buffer.byteLength("https://no.run");
  assert.equal(inspectGif(Buffer.concat([gif.subarray(0, -1), comment, gif.subarray(-1)])).frames, 1);
  const alternative = Buffer.from(gif); alternative.write("ANIMEXTS1.0", 22);
  assert.equal(inspectGif(alternative).frames, 1);
});

test("GIF impõe resolução, orçamento de canvas, frames e duração antes de renderizar", () => {
  assert.equal(inspectGif(gifFixture({ frames: 240 })).frames, 240);
  assert.equal(inspectGif(gifFixture({ width: 1920, height: 1080, frames: 77 })).frames, 77);
  assert.equal(inspectGif(gifFixture({ frames: 60, delays: Array(60).fill(100) })).durationMs, 60_000);
  for (const bytes of [
    gifFixture({ width: 1921 }), gifFixture({ height: 1081 }), gifFixture({ width: 0 }),
    gifFixture({ frames: 241 }), gifFixture({ width: 1920, height: 1080, frames: 78 }),
    gifFixture({ frames: 61, delays: Array(61).fill(100) }), gifFixture({ delays: [65535] }),
    Buffer.alloc(THEME_LIMITS.gif + 1),
  ]) assert.throws(() => inspectGif(bytes));
});

test("GIF recusa truncamentos, extensões externas, controles interativos e subblocos forjados", () => {
  const gif = gifFixture(), image = gif.indexOf(0x2c), control = gif.indexOf(Buffer.from([0x21, 0xf9]));
  const mutate = (change) => { const bytes = Buffer.from(gif); change(bytes); return bytes; };
  const malformed = [
    gif.subarray(0, 12), gif.subarray(0, -1), Buffer.concat([gif, Buffer.from([0])]), gifFixture({ frames: 0 }),
    mutate((b) => { b[0] = 0; }), mutate((b) => { b[11] = 2; }),
    mutate((b) => { b[image + 1] = 1; }), mutate((b) => { b[image + 5] = 0; }),
    mutate((b) => { b[image + 9] |= 0x08; }), mutate((b) => { b[image + 10] = 1; }),
    mutate((b) => { b[image + 10] = 9; }), mutate((b) => { b[image + 11] = 255; }),
    mutate((b) => { b[image + 11] = 0; }), mutate((b) => { b[control + 2] = 3; }),
    mutate((b) => { b[control + 3] = 2; }), mutate((b) => { b[control + 3] = 16; }),
    mutate((b) => { b[control + 3] = 32; }), mutate((b) => { b[control + 3] = 5; b[control + 6] = 2; }),
    mutate((b) => { b.write("UNSAFEAPP00", 22); }), mutate((b) => { b[33] = 4; }),
    Buffer.concat([gif.subarray(0, image), gif.subarray(control, control + 8), gif.subarray(image)]),
    Buffer.concat([gif.subarray(0, -1), gif.subarray(control, control + 8), gif.subarray(-1)]),
    Buffer.concat([gif.subarray(0, -1), Buffer.from([0x21, 0x01, 0]), gif.subarray(-1)]),
    Buffer.concat([gif.subarray(0, -1), Buffer.from([0x21, 0x99, 0]), gif.subarray(-1)]),
  ];
  const noPalette = Buffer.concat([gif.subarray(0, 13), gif.subarray(19)]); noPalette[10] = 0;
  malformed.push(noPalette);
  for (const bytes of malformed) assert.throws(() => inspectGif(bytes));
});

test("GIF embutido fica restrito ao fundo; logo, ícones e elementos continuam PNG", () => {
  const asset = { mime: "image/gif", data: gifFixture({ frames: 2 }).toString("base64") };
  assert.equal(read((x) => { x.assets.scene = asset; x.media.background = "scene"; }).assets.scene.mime, "image/gif");
  for (const mutate of [
    (x) => { x.media.logo = "scene"; },
    (x) => { x.icons.instances = "asset:scene"; },
    (x) => { x.decorations = [{ type: "image", text: "GIF", asset: "scene", align: "center" }]; },
  ]) assert.throws(() => read((x) => { x.assets.scene = asset; mutate(x); }), /PNG/);
  assert.throws(() => read((x) => { x.assets.scene = { mime: "image/gif", data: Buffer.from("<html>script</html>").toString("base64") }; }), /GIF/);
});

test("novos limites de moldura, layout, transição e elementos são validados inclusive nas bordas", () => {
  const ranges = [
    ["window", "titlebarHeight", 36, 64], ["window", "borderWidth", 0, 3], ["window", "outerRadius", 0, 24], ["window", "contentInset", 0, 24],
    ["layout", "sidebarWidth", 180, 300], ["layout", "decorationHeight", 80, 400], ["effects", "transitionMs", 100, 700],
  ];
  for (const [object, field, min, max] of ranges) {
    for (const value of [min, max]) assert.equal(read((x) => { x[object][field] = value; })[object][field], value);
    for (const value of [min - 0.1, max + 0.1, null, "12px"]) assert.throws(() => read((x) => { x[object][field] = value; }), /número/);
  }
  for (const [field, min, max] of [["x", 0, 100], ["y", 0, 100], ["width", 8, 100], ["height", 24, 160], ["fontSize", 12, 56]]) {
    for (const value of [min, max]) assert.equal(read((x) => { x.decorations = [{ type: "text", text: "a", asset: null, align: "center", [field]: value }]; }).decorations[0][field], value);
    for (const value of [min - 0.1, max + 0.1, null]) assert.throws(() => read((x) => { x.decorations = [{ type: "text", text: "a", asset: null, align: "center", [field]: value }]; }), /número/);
  }
  for (const transition of ["fade", "slide", "zoom", "none"]) assert.equal(read((x) => { x.effects.transition = transition; }).effects.transition, transition);
  assert.throws(() => read((x) => { x.effects.transition = "url(https://evil.example)"; }));
  assert.throws(() => read((x) => { x.window = null; }), /objeto/);
  assert.throws(() => read((x) => { x.window.titlebarText = null; }), /#RRGGBB/);
  assert.throws(() => read((x) => { x.window.css = "position:fixed"; }), /não permitido/);
});

test("ordem das áreas é uma permutação exata e a barra de título conserva contraste", () => {
  assert.deepEqual(read((x) => { x.layout.sectionOrder = ["content", "header", "decorations"]; x.layout.statusSide = "left"; }).layout.sectionOrder, ["content", "header", "decorations"]);
  for (const order of [["header", "header", "content"], ["header", "content"], ["header", "decorations", "security"], null]) assert.throws(() => read((x) => { x.layout.sectionOrder = order; }));
  assert.throws(() => read((x) => { x.layout.statusSide = "top"; }));
  assert.throws(() => read((x) => { x.window.titlebarText = x.window.titlebarColor; }), /contraste/);
});

test("JSONC mantém strings e aceita comentários de linha/bloco sem executar seu conteúdo", () => {
  const theme = fixture();
  theme.decorations = [{ type: "text", text: 'https://site.example/* texto */ e aspas "x" e barra \\', asset: null, align: "center", x: 10, y: 20, width: 60, height: 80, fontSize: 24 }];
  const code = JSON.stringify(theme, null, 2);
  const commented = "// início\r\n/* globalThis.auroraThemeCodeExecuted = true; [ { */\r\n" + code.replace('"schemaVersion": 1,', '"schemaVersion": /* versão\r\n fixa */ 1, // comentário\r\n') + "\r\n// fim";
  assert.deepEqual(parseTheme(commented), theme);
  assert.equal(globalThis.auroraThemeCodeExecuted, undefined);
  assert.throws(() => parseTheme(commented + "\n/* não fecha"), /comentário.*sem fechamento/);
  for (const replacement of ["1/* nada */0", "1/* nada */.0", '(() => { globalThis.auroraThemeCodeExecuted = true; return 1; })()']) {
    assert.throws(() => parseTheme(code.replace('"schemaVersion": 1,', `"schemaVersion": ${replacement},`)), /JSON inválido/);
  }
  assert.equal(globalThis.auroraThemeCodeExecuted, undefined);
  assert.throws(() => parseTheme(code.replace('"schemaVersion": 1,', '"schema"/* nada */"Version": 1,')), /JSON inválido/);
  assert.throws(() => parseTheme(code.replace(/}\s*$/, ",}")), /JSON inválido/);
  assert.throws(() => parseTheme("/*" + "x".repeat(THEME_LIMITS.json) + "*/" + code), /34 MiB/);
});

test("texto recusa surrogates isolados que o armazenamento Rust não pode representar", () => {
  for (const value of ["\ud800", "\udfff", "antes\ud800depois", "\udfff\ud800"]) {
    assert.throws(() => read((x) => { x.name = value; }), /Unicode válido/);
    assert.throws(() => read((x) => { x.decorations = [{ type: "text", text: value, asset: null, align: "center" }]; }), /Unicode válido/);
  }
  assert.equal(read((x) => { x.name = "Jardim 🌸"; }).name, "Jardim 🌸");
});

test("código comentado explica os campos e preserva valores e recursos ao reimportar", () => {
  const theme = read((x) => { x.assets.logo = { mime: "image/png", data: png }; x.media.logo = "logo"; x.decorations = [{ type: "text", text: "Canal", asset: null, align: "right" }]; });
  const code = formatThemeCode(theme);
  assert.deepEqual(parseTheme(code), theme);
  for (const phrase of ["JSONC", "36 a 64 pixels", "180 a 300 pixels", "100 a 700 milissegundos", "espaço livre", "8 a 100%", "12 a 56 pixels", "base64 puro", "image/png"]) assert.ok(code.includes(phrase), phrase);
  assert.ok(code.includes(png));
  const intermediate = { ...theme, name: "" };
  assert.doesNotThrow(() => formatThemeCode(intermediate));
  assert.throws(() => parseTheme(formatThemeCode(intermediate)), /name/);
});

test("HTML, JS, CSS, comando e propriedades extras não se tornam capacidades", () => {
  for (const key of ["html", "css", "script", "onClick", "invoke", "permissions", "__proto__", "constructor"]) {
    assert.throws(() => parseTheme(JSON.stringify(fixture()).replace('"schemaVersion":1', `"schemaVersion":1,"${key}":"evil"`)), /não permitido/);
  }
  for (const mutate of [
    (x) => { x.colors.accent = "url(https://evil.example)"; },
    (x) => { x.colors.accent = "red;position:fixed"; },
    (x) => { x.typography.font = "https://evil.example/font.woff"; },
    (x) => { x.media.background = "https://evil.example/track.png"; },
    (x) => { x.layout.navOrder = ["instances", "discover", "appearance", "shell"]; },
    (x) => { x.effects.background = "javascript:alert(1)"; },
    (x) => { x.layout.onclick = "invoke('launch')"; },
  ]) assert.throws(() => read(mutate));
  const displayed = '<img src=x onerror="alert(1)">';
  assert.equal(read((x) => { x.decorations = [{ type: "text", text: displayed, asset: null, align: "center" }]; }).decorations[0].text, displayed);
});

test("limites de números, nomes, profundidade de estrutura e IDs especiais são rejeitados", () => {
  for (const id of ["../theme", "C:/theme", "__proto__", "prototype", "constructor", "UpperCase", "x".repeat(49)]) assert.throws(() => read((x) => { x.id = id; }), /identificador/);
  for (const mutate of [
    (x) => { x.typography.scale = 10; },
    (x) => { x.effects.speed = 0; },
    (x) => { x.effects.glass = 100; },
    (x) => { x.effects.intensity = -1; },
    (x) => { x.layout.radius = 25; },
    (x) => { x.name = "🌸".repeat(61); },
    (x) => { x.name = "hidden\u0000"; },
    (x) => { x.layout.navOrder = ["instances", "instances", "appearance", "java"]; },
    (x) => { x.colors = []; },
    (x) => { x.decorations = Array.from({ length: 7 }, () => ({ type: "text", text: "a", asset: null, align: "center" })); },
  ]) assert.throws(() => read(mutate));
  assert.equal(read((x) => { x.name = "🌸".repeat(60); }).name.length, 120);
  assert.throws(() => parseTheme("[[]]"), /objeto/);
  assert.throws(() => parseTheme('{"schemaVersion":1,"broken":'), /JSON inválido/);
  assert.throws(() => parseTheme(" ".repeat(THEME_LIMITS.json + 1)), /34 MiB/);
});

test("cores de texto e de ação precisam de contraste legível", () => {
  assert.throws(() => read((x) => { x.colors.text = x.colors.panel; }), /contraste/);
  assert.throws(() => read((x) => { x.colors.accentText = x.colors.accent; }), /contraste/);
  assert.equal(read((x) => { x.colors.text = "#FFFFFF"; x.colors.panel = "#000000"; }).colors.text, "#ffffff");
});

test("a estrutura é limitada antes de JSON.parse alocar listas ou objetos gigantes", () => {
  assert.throws(() => parseTheme('{"extra":[' + "[],".repeat(100_000) + "[]]}"), /estrutura JSON/);
  assert.throws(() => parseTheme('{"extra":[' + "0,".repeat(100_000) + "0]}"), /itens demais/);
  assert.throws(() => parseTheme('['.repeat(9) + '0' + ']'.repeat(9)), /profunda/);
  assert.equal(read((x) => { x.decorations = [{ type: "text", text: 'Aspas: "x"; estruturas: [] {} , : e barra \\', asset: null, align: "left" }]; }).decorations.length, 1);
});

test("PNG embutido pode preencher fundo, logo, ícone e decoração sem URL", () => {
  const parsed = read((x) => {
    x.assets.logo = { mime: "image/png", data: png };
    x.media = { background: "logo", logo: "logo" };
    x.icons.instances = "asset:logo";
    x.decorations = [{ type: "image", text: "Meu canal", asset: "logo", align: "right" }];
  });
  assert.equal(parsed.media.logo, "logo");
  assert.equal(parsed.icons.instances, "asset:logo");
  for (const mutate of [
    (x) => { x.media.logo = "missing"; },
    (x) => { x.icons.instances = "asset:missing"; },
    (x) => { x.assets.image = { mime: "image/svg+xml", data: btoa("<svg onload='evil()'/>") }; },
    (x) => { x.assets.image = { mime: "image/png", data: `data:image/png;base64,${png}` }; },
    (x) => { x.assets.image = { mime: "image/png", data: btoa("not a PNG") }; },
    (x) => { x.assets.image = { mime: "image/png", data: "AB==" }; },
    (x) => { x.assets.image = { mime: "image/png", data: png.replaceAll("A", "_") }; },
    (x) => { x.assets.image = { mime: "image/png", data: png, path: "C:/secret" }; },
    (x) => { x.assets = Object.fromEntries(Array.from({ length: 5 }, (_, i) => [`image-${i}`, { mime: "image/png", data: png }])); },
  ]) assert.throws(() => read(mutate));
});

test("dimensões gigantes de PNG são rejeitadas antes do decoder", () => {
  const bytes = Buffer.from(png, "base64");
  bytes.writeUInt32BE(4097, 16);
  assert.throws(() => read((x) => { x.assets.image = { mime: "image/png", data: bytes.toString("base64") }; }), /4096/);
  bytes.writeUInt32BE(4000, 16); bytes.writeUInt32BE(4000, 20);
  assert.throws(() => read((x) => { x.assets.image = { mime: "image/png", data: bytes.toString("base64") }; }), /milhões/);
});

test("APNG é recusado por chunks, preservando PNG estático com texto semelhante", () => {
  const bytes = Buffer.from(png, "base64");
  function chunk(type, data) {
    const result = Buffer.alloc(12 + data.length);
    result.writeUInt32BE(data.length); result.write(type, 4); data.copy(result, 8);
    return result;
  }
  for (const type of ["acTL", "fcTL", "fdAT"]) {
    const animated = Buffer.concat([bytes.subarray(0, 33), chunk(type, Buffer.alloc(8)), bytes.subarray(33)]);
    assert.throws(() => read((x) => { x.assets.image = { mime: "image/png", data: animated.toString("base64") }; }), /estático|APNG/);
  }
  const still = Buffer.concat([bytes.subarray(0, 33), chunk("tEXt", Buffer.from("Comment\0acTL is text")), bytes.subarray(33)]);
  assert.equal(read((x) => { x.assets.image = { mime: "image/png", data: still.toString("base64") }; }).assets.image.mime, "image/png");
  for (const malformed of [bytes.subarray(0, 33), Buffer.concat([bytes, Buffer.from("tail")])]) {
    assert.throws(() => read((x) => { x.assets.image = { mime: "image/png", data: malformed.toString("base64") }; }), /estático/);
  }
});

test("MP4/WebM exigem assinatura e PNG é obrigatório para elementos pequenos", () => {
  const mp4 = Buffer.alloc(24); mp4.writeUInt32BE(24); mp4.write("ftypisom", 4); mp4.write("isom", 16);
  const webm = Buffer.from([0x1a, 0x45, 0xdf, 0xa3, 0x87, 0x42, 0x82, 0x84, 0x77, 0x65, 0x62, 0x6d]);
  for (const [mime, data] of [["video/mp4", mp4], ["video/webm", webm]]) {
    assert.equal(read((x) => { x.assets.film = { mime, data: data.toString("base64") }; x.media.background = "film"; }).assets.film.mime, mime);
    assert.throws(() => read((x) => { x.assets.film = { mime, data: data.toString("base64") }; x.media.logo = "film"; }), /PNG/);
    assert.throws(() => read((x) => { x.assets.film = { mime, data: data.toString("base64") }; x.icons.java = "asset:film"; }), /PNG/);
  }
  mp4.write("html", 4);
  assert.throws(() => read((x) => { x.assets.film = { mime: "video/mp4", data: mp4.toString("base64") }; }), /MP4/);
  assert.throws(() => read((x) => { x.assets.film = { mime: "video/webm", data: Buffer.from([0x1a, 0x45, 0xdf, 0xa3, 0, 0, 0, 0]).toString("base64") }; }), /WebM/);
  for (const forged of [
    [0x1a, 0x45, 0xdf, 0xa3, 0x80, 0x42, 0x82, 0x84, 0x77, 0x65, 0x62, 0x6d],
    [0x1a, 0x45, 0xdf, 0xa3, 0xff, 0x42, 0x82, 0x84, 0x77, 0x65, 0x62, 0x6d],
    [0x1a, 0x45, 0xdf, 0xa3, 0x8e, 0x42, 0x82, 0x84, 0x77, 0x65, 0x62, 0x6d, 0x42, 0x82, 0x84, 0x77, 0x65, 0x62, 0x6d],
  ]) assert.throws(() => read((x) => { x.assets.film = { mime: "video/webm", data: Buffer.from(forged).toString("base64") }; }), /WebM/);
});

test("vídeos grandes e o orçamento total são recusados com trabalho limitado", () => {
  const big = Buffer.alloc(THEME_LIMITS.video + 1); big.writeUInt32BE(24); big.write("ftypisom", 4);
  assert.throws(() => read((x) => { x.assets.film = { mime: "video/mp4", data: big.toString("base64") }; }), /20 MiB/);
  const half = Buffer.alloc(12.5 * 1024 * 1024); half.writeUInt32BE(24); half.write("ftypisom", 4);
  assert.throws(() => read((x) => { x.assets = { a: { mime: "video/mp4", data: half.toString("base64") }, b: { mime: "video/mp4", data: half.toString("base64") } }; }), /24 MiB/);
});

test("variáveis CSS vêm só de valores tipados", () => {
  const variables = themeVariables(fixture());
  assert.ok(Object.keys(variables).every((key) => key.startsWith("--theme-")));
  assert.equal(variables["--theme-glass"], "10px");
  assert.equal(variables["--theme-accent"], fixture().colors.accent);
  assert.equal(variables["--theme-titlebar-height"], "44px");
  assert.equal(variables["--theme-sidebar-width"], "240px");
  assert.equal(variables["--theme-transition-duration"], "300ms");
});
