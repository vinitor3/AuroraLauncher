import assert from "node:assert/strict";
import test from "node:test";
import { DEFAULT_THEME, THEME_EXAMPLES, THEME_LIMITS, parseTheme, themeVariables, buildThemeAiPrompt } from "./model.ts";

const fixture = () => structuredClone(THEME_EXAMPLES[0]);
function read(change) { const theme = fixture(); change?.(theme); return parseTheme(JSON.stringify(theme)); }
const png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a6f8AAAAASUVORK5CYII=";

test("os exemplos passam pelo schema fechado e o padrão tem id reservado", () => {
  for (const example of THEME_EXAMPLES) assert.deepEqual(parseTheme(JSON.stringify(example)), example);
  assert.throws(() => parseTheme(JSON.stringify(DEFAULT_THEME)), /reservado/);
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

test("variáveis CSS vêm só de valores tipados e instruções exportam o contrato local", () => {
  const variables = themeVariables(fixture());
  assert.ok(Object.keys(variables).every((key) => key.startsWith("--theme-")));
  assert.equal(variables["--theme-glass"], "10px");
  assert.equal(variables["--theme-accent"], fixture().colors.accent);
  const prompt = buildThemeAiPrompt(fixture());
  assert.match(prompt, /SOMENTE com um objeto JSON/);
  assert.match(prompt, /não executa código/);
  assert.match(prompt, /10 segundos/);
  assert.match(prompt, /4\.5:1/);
  assert.ok(prompt.includes('"schemaVersion": 1'));
});
