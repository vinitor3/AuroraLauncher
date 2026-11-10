/** Theme documents are untrusted data. Never turn their contents into markup or code. */
export type NavId = "instances" | "discover" | "appearance" | "java";
export type ThemeSectionId = "header" | "decorations" | "content";
export type ThemeAsset = { mime: "image/png" | "image/gif" | "video/mp4" | "video/webm"; data: string };
export type GifInfo = { width: number; height: number; frames: number; durationMs: number };
export type AuroraTheme = {
  schemaVersion: 1;
  id: string;
  name: string;
  colors: { background: string; panel: string; text: string; muted: string; accent: string; accentText: string; border: string };
  window: { titlebarColor: string; titlebarText: string; frameColor: string; titlebarHeight: number; borderWidth: number; outerRadius: number; contentInset: number };
  cursor: { style: "system" | "pixel" | "ring" | "crosshair"; color: string; size: number; trail: "none" | "sparkle" };
  typography: { font: "system" | "rounded" | "serif" | "mono"; scale: number };
  layout: { sidebar: "left" | "right"; density: "comfortable" | "compact"; navOrder: NavId[]; radius: number; sidebarWidth: number; statusSide: "right" | "left"; sectionOrder: ThemeSectionId[]; decorationHeight: number };
  effects: { background: "none" | "aurora" | "water" | "sakura"; intensity: number; speed: number; glass: number; transition: "fade" | "slide" | "zoom" | "none"; transitionMs: number };
  assets: Record<string, ThemeAsset>;
  media: { background: string | null; logo: string | null };
  icons: Record<NavId, string>;
  decorations: Array<{ type: "text" | "image"; text: string; asset: string | null; align: "left" | "center" | "right"; x: number; y: number; width: number; height: number; fontSize: number }>;
};
export type ThemeLibrary = {
  themes: AuroraTheme[];
  selectedThemeId: string | null;
  bindings: Record<string, string>;
  automatic: boolean;
  recovered: boolean;
  persistent: boolean;
};
export type ThemePreview = { token: string; deadlineMs: number };

const MIB = 1024 * 1024;
export const THEME_LIMITS = { json: 34 * MIB, png: 2 * MIB, gif: 24 * MIB, video: 20 * MIB, assets: 24 * MIB, assetCount: 4 } as const;
export const NAV_IDS: readonly NavId[] = ["instances", "discover", "appearance", "java"];
export const THEME_SECTION_IDS: readonly ThemeSectionId[] = ["header", "decorations", "content"];
export const THEME_ICON_NAMES = ["default", "star", "leaf", "moon", "heart", "gamepad", "sparkles"] as const;
export const THEME_WINDOW_DEFAULTS: AuroraTheme["window"] = { titlebarColor: "#171224", titlebarText: "#f8f6ff", frameColor: "#0b0812", titlebarHeight: 44, borderWidth: 1, outerRadius: 20, contentInset: 16 };
export const THEME_CURSOR_DEFAULTS: AuroraTheme["cursor"] = { style: "system", color: "#f2efff", size: 24, trail: "none" };
const ID = /^[a-z0-9][a-z0-9-]{0,47}$/;
const BAD_IDS = new Set(["constructor", "prototype", "__proto__"]);
const FONT_FAMILIES = {
  system: 'Inter, ui-sans-serif, system-ui, sans-serif',
  rounded: '"Arial Rounded MT Bold", "Trebuchet MS", ui-sans-serif, sans-serif',
  serif: 'Georgia, "Times New Roman", serif',
  mono: '"Cascadia Code", Consolas, ui-monospace, monospace',
} as const;

function invalid(path: string, message: string): never { throw new Error(`${path}: ${message}`); }
function object(value: unknown, path: string): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) invalid(path, "use um objeto JSON.");
  return value as Record<string, unknown>;
}
function closed(value: unknown, keys: readonly string[], path: string, optional: readonly string[] = []): Record<string, unknown> {
  const result = object(value, path);
  for (const key of Object.keys(result)) if (!keys.includes(key) && !optional.includes(key)) invalid(path, `campo não permitido: ${key.slice(0, 60)}.`);
  for (const key of keys) if (!Object.hasOwn(result, key)) invalid(path, `campo obrigatório: ${key}.`);
  return result;
}
function fallback(value: unknown, defaultValue: unknown): unknown { return value === undefined ? defaultValue : value; }
function text(value: unknown, path: string, max: number, allowEmpty = false): string {
  if (typeof value !== "string" || (!allowEmpty && !value.trim()) || Array.from(value).length > max || /[\u0000-\u001f\u007f-\u009f]/.test(value) || /[\ud800-\udfff]/u.test(value)) {
    invalid(path, `use texto ${allowEmpty ? "de até" : "não vazio de até"} ${max} caracteres, sem controles e com Unicode válido.`);
  }
  return value;
}
function identifier(value: unknown, path: string): string {
  if (typeof value !== "string" || !ID.test(value) || BAD_IDS.has(value)) invalid(path, "use um identificador de até 48 letras minúsculas, números e hífens.");
  return value;
}
function choice<T extends string>(value: unknown, choices: readonly T[], path: string): T {
  if (typeof value !== "string" || !choices.includes(value as T)) invalid(path, `opção permitida: ${choices.join(", ")}.`);
  return value as T;
}
function number(value: unknown, min: number, max: number, path: string): number {
  if (typeof value !== "number" || !Number.isFinite(value) || value < min || value > max) invalid(path, `use um número de ${min} a ${max}.`);
  return value;
}
function color(value: unknown, path: string): string {
  if (typeof value !== "string" || !/^#[0-9a-fA-F]{6}$/.test(value)) invalid(path, "use uma cor no formato #RRGGBB.");
  return value.toLowerCase();
}
function luminance(value: string): number {
  const channels = [1, 3, 5].map((offset) => {
    const channel = Number.parseInt(value.slice(offset, offset + 2), 16) / 255;
    return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
}
function contrast(foreground: string, background: string, path: string): void {
  const a = luminance(foreground), b = luminance(background);
  if ((Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05) < 4.5) invalid(path, "o contraste precisa ser de pelo menos 4,5:1 para manter a leitura.");
}
function u32(bytes: Uint8Array, offset: number): number {
  return ((bytes[offset] * 0x1000000) + (bytes[offset + 1] << 16) + (bytes[offset + 2] << 8) + bytes[offset + 3]) >>> 0;
}
function staticPng(bytes: Uint8Array): boolean {
  let offset = 8;
  let image = false;
  while (offset + 12 <= bytes.length) {
    const length = u32(bytes, offset);
    const end = offset + 12 + length;
    if (end > bytes.length) return false;
    const type = String.fromCharCode(...bytes.subarray(offset + 4, offset + 8));
    if (["acTL", "fcTL", "fdAT"].includes(type) || (type === "IHDR" && offset !== 8)) return false;
    if (type === "IDAT") image = true;
    if (type === "IEND") return image && length === 0 && end === bytes.length;
    offset = end;
  }
  return false;
}
function ebmlNumber(bytes: Uint8Array, offset: number, identifier: boolean): { value: bigint; length: number } | null {
  const first = bytes[offset];
  if (!first) return null;
  const length = Math.clz32(first) - 23;
  if (length > (identifier ? 4 : 8) || offset + length > bytes.length) return null;
  let value = BigInt(identifier ? first : first & (0xff >> length));
  for (let i = 1; i < length; i += 1) value = (value << 8n) | BigInt(bytes[offset + i]);
  if (!identifier && value === (1n << BigInt(7 * length)) - 1n) return null;
  return { value, length };
}
function validWebm(bytes: Uint8Array): boolean {
  if (bytes.length < 12 || ![0x1a, 0x45, 0xdf, 0xa3].every((byte, index) => bytes[index] === byte)) return false;
  const length = ebmlNumber(bytes, 4, false);
  if (!length || length.value > 4096n) return false;
  let cursor = 4 + length.length;
  const end = cursor + Number(length.value);
  if (end > bytes.length) return false;
  let found = false;
  while (cursor < end) {
    const id = ebmlNumber(bytes.subarray(0, end), cursor, true);
    if (!id) return false;
    cursor += id.length;
    const size = ebmlNumber(bytes.subarray(0, end), cursor, false);
    if (!size || size.value > BigInt(end)) return false;
    cursor += size.length;
    const elementEnd = cursor + Number(size.value);
    if (elementEnd > end) return false;
    if (id.value === 0x4282n) {
      if (found || String.fromCharCode(...bytes.subarray(cursor, elementEnd)) !== "webm") return false;
      found = true;
    }
    cursor = elementEnd;
  }
  return found;
}

/** Inspect the bounded GIF container; the browser separately verifies LZW decoding. */
export function inspectGif(bytes: Uint8Array): GifInfo {
  const fail = (message: string): never => invalid("GIF", message);
  if (bytes.length > THEME_LIMITS.gif) fail("o arquivo excede 24 MiB.");
  if (bytes.length < 13 || !["GIF87a", "GIF89a"].includes(String.fromCharCode(...bytes.subarray(0, 6)))) fail("assinatura ou cabeçalho inválido.");
  const read16 = (offset: number) => bytes[offset] | (bytes[offset + 1] << 8);
  const width = read16(6), height = read16(8), pixels = width * height;
  if (width < 1 || height < 1 || width > 1920 || height > 1080 || pixels > 2_073_600) fail("use um GIF de até 1920×1080 e 2.073.600 pixels.");
  let cursor = 13;
  const need = (count: number) => { if (cursor + count > bytes.length) fail("arquivo truncado ou subbloco incompleto."); };
  const globalColors = bytes[10] & 0x80 ? 2 ** ((bytes[10] & 7) + 1) : 0;
  if (globalColors) {
    if (bytes[11] >= globalColors) fail("índice de fundo fora da paleta global.");
    need(globalColors * 3); cursor += globalColors * 3;
  }
  function subBlocks(): number {
    let payload = 0;
    while (true) {
      need(1);
      const count = bytes[cursor++];
      if (count === 0) return payload;
      need(count); cursor += count; payload += count;
    }
  }
  let frames = 0, durationMs = 0, delayMs = 0;
  let pendingControl = false;
  let transparency: number | null = null;
  while (cursor < bytes.length) {
    const tag = bytes[cursor++];
    if (tag === 0x3b) {
      if (cursor !== bytes.length || frames < 1 || pendingControl) fail("final inválido: use frames completos, sem dados após o trailer.");
      return { width, height, frames, durationMs };
    }
    if (tag === 0x21) {
      need(1);
      const extension = bytes[cursor++];
      if (extension === 0xf9) {
        need(6);
        const flags = bytes[cursor + 1];
        if (pendingControl || bytes[cursor] !== 4 || bytes[cursor + 5] !== 0 || (flags & 0xe0) !== 0 || (flags & 2) !== 0 || ((flags >> 2) & 7) > 3) fail("controle de frame inválido ou interativo.");
        delayMs = read16(cursor + 2) * 10;
        transparency = flags & 1 ? bytes[cursor + 4] : null;
        pendingControl = true;
        cursor += 6;
      } else if (extension === 0xff) {
        need(12);
        const application = String.fromCharCode(...bytes.subarray(cursor + 1, cursor + 12));
        if (bytes[cursor] !== 11 || !["NETSCAPE2.0", "ANIMEXTS1.0"].includes(application)) fail("extensão de aplicação não permitida; use somente looping GIF padrão.");
        cursor += 12;
        need(5);
        if (bytes[cursor] !== 3 || bytes[cursor + 1] !== 1 || bytes[cursor + 4] !== 0) fail("bloco de looping inválido.");
        cursor += 5;
      } else if (extension === 0xfe) subBlocks();
      else fail("extensão não permitida; texto interativo e extensões externas não são aceitos.");
      continue;
    }
    if (tag !== 0x2c) fail("bloco de imagem inválido.");
    need(9);
    const left = read16(cursor), top = read16(cursor + 2), frameWidth = read16(cursor + 4), frameHeight = read16(cursor + 6), flags = bytes[cursor + 8];
    if (frameWidth < 1 || frameHeight < 1 || left + frameWidth > width || top + frameHeight > height || (flags & 0x18) !== 0) fail("frame ultrapassa a área do GIF ou tem campos reservados inválidos.");
    cursor += 9;
    let colors = globalColors;
    if (flags & 0x80) { colors = 2 ** ((flags & 7) + 1); need(colors * 3); cursor += colors * 3; }
    if (!colors || (transparency !== null && transparency >= colors)) fail("paleta ausente ou índice transparente fora da paleta.");
    need(1);
    const lzw = bytes[cursor++];
    if (lzw < 2 || lzw > 8 || subBlocks() === 0) fail("dados LZW ausentes ou código mínimo fora de 2–8.");
    frames += 1;
    durationMs += Math.max(delayMs, 40);
    if (frames > 240 || pixels * frames > 160_000_000 || durationMs > 60_000) fail("excede 240 frames, 160 milhões de pixels acumulados ou 60 segundos por ciclo.");
    pendingControl = false; delayMs = 0; transparency = null;
  }
  return fail("arquivo truncado: trailer final ausente.");
}

function asset(value: unknown, path: string): { result: ThemeAsset; size: number } {
  const raw = closed(value, ["mime", "data"], path);
  const mime = choice(raw.mime, ["image/png", "image/gif", "video/mp4", "video/webm"] as const, `${path}.mime`);
  const encoded = raw.data;
  const limit = mime === "image/png" ? THEME_LIMITS.png : mime === "image/gif" ? THEME_LIMITS.gif : THEME_LIMITS.video;
  if (typeof encoded !== "string" || !encoded || encoded.length % 4 !== 0 || encoded.length > Math.ceil(limit / 3) * 4) {
    invalid(path, `recurso inválido; use base64 puro, até ${limit / MIB} MiB, sem URL nem prefixo data:.`);
  }
  const padding = encoded.endsWith("==") ? 2 : encoded.endsWith("=") ? 1 : 0;
  const body = encoded.slice(0, encoded.length - padding);
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  if (/[^A-Za-z0-9+/]/.test(body) || (padding === 2 && alphabet.indexOf(body.at(-1) ?? "") % 16 !== 0) || (padding === 1 && alphabet.indexOf(body.at(-1) ?? "") % 4 !== 0)) invalid(path, "base64 inválido ou não canônico.");
  const size = (encoded.length / 4) * 3 - padding;
  if (size > limit) invalid(path, `recurso maior que ${limit / MIB} MiB.`);
  // Decode only the header here. Browser media decoding is separate and can fail safely.
  const header = Uint8Array.from(atob(encoded.slice(0, 5480)), (char) => char.charCodeAt(0));
  if (mime === "image/png") {
    if (header.length < 33 || ![137, 80, 78, 71, 13, 10, 26, 10].every((byte, index) => header[index] === byte) || u32(header, 8) !== 13 || String.fromCharCode(...header.slice(12, 16)) !== "IHDR") invalid(path, "assinatura PNG inválida.");
    const width = u32(header, 16), height = u32(header, 20);
    if (width < 1 || height < 1 || width > 4096 || height > 4096 || width * height > 8_000_000) invalid(path, "PNG excede 4096 px por dimensão ou 8 milhões de pixels.");
    const bytes = encoded.length <= 5480 ? header : Uint8Array.from(atob(encoded), (char) => char.charCodeAt(0));
    if (!staticPng(bytes)) invalid(path, "use um PNG estático válido, sem animação APNG. Para animar, escolha um efeito interno ou vídeo.");
  } else if (mime === "image/gif") {
    const width = header[6] | (header[7] << 8), height = header[8] | (header[9] << 8);
    if (header.length < 13 || !["GIF87a", "GIF89a"].includes(String.fromCharCode(...header.subarray(0, 6))) || width < 1 || height < 1 || width > 1920 || height > 1080 || width * height > 2_073_600) invalid(path, "assinatura ou dimensões GIF inválidas.");
    inspectGif(encoded.length <= 5480 ? header : Uint8Array.from(atob(encoded), (char) => char.charCodeAt(0)));
  } else if (mime === "video/mp4") {
    const brands = ["isom", "iso2", "iso3", "iso4", "iso5", "iso6", "mp41", "mp42", "avc1", "M4V "];
    if (header.length < 16 || String.fromCharCode(...header.slice(4, 8)) !== "ftyp" || u32(header, 0) < 16 || u32(header, 0) > size || u32(header, 0) % 4 !== 0 || !brands.includes(String.fromCharCode(...header.slice(8, 12)))) invalid(path, "assinatura ou marca MP4 inválida.");
  } else if (!validWebm(header)) invalid(path, "assinatura ou DocType WebM inválido.");
  return { result: { mime, data: encoded }, size };
}

function quotedEnd(source: string, opening: number): number {
  let quote = source.indexOf('"', opening + 1);
  while (quote >= 0) {
    let backslashes = 0;
    for (let before = quote - 1; before >= 0 && source[before] === "\\"; before -= 1) backslashes += 1;
    if (backslashes % 2 === 0) return quote;
    quote = source.indexOf('"', quote + 1);
  }
  return invalid("Tema", "JSON inválido: texto sem fechamento.");
}

/** Comments become whitespace, so a comment can never join two JSON tokens. */
function withoutComments(source: string): string {
  const chunks: string[] = [];
  let parts: string[] = [];
  let start = 0;
  function append(value: string) {
    parts.push(value);
    // Bound temporary arrays even for a document consisting of many comments.
    if (parts.length >= 128) { chunks.push(parts.join("")); parts = []; }
  }
  for (let index = 0; index < source.length; index += 1) {
    if (source[index] === '"') { index = quotedEnd(source, index); continue; }
    if (source[index] !== "/") continue;
    let end: number;
    if (source[index + 1] === "/") {
      end = index + 2;
      while (end < source.length && source[end] !== "\r" && source[end] !== "\n") end += 1;
    } else if (source[index + 1] === "*") {
      end = source.indexOf("*/", index + 2);
      if (end < 0) invalid("Tema", "comentário JSONC de bloco sem fechamento */.");
      end += 2;
    } else continue;
    append(source.slice(start, index));
    append(source.slice(index, end).replace(/[^\r\n]/g, " "));
    start = end;
    index = end - 1;
  }
  if (start === 0) return source;
  append(source.slice(start));
  if (parts.length) chunks.push(parts.join(""));
  return chunks.join("");
}

/** Bound structure before JSON.parse can allocate a large tree. Embedded bytes are strings. */
function checkJsonStructure(source: string): void {
  let depth = 0, containers = 0, separators = 0;
  for (let index = 0; index < source.length; index += 1) {
    const character = source[index];
    if (character === '"') {
      index = quotedEnd(source, index);
    } else if (character === "{" || character === "[") {
      depth += 1; containers += 1;
      if (depth > 8 || containers > 128) invalid("Tema", "a estrutura JSON é muito extensa ou profunda; use apenas o formato Aurora v1.");
    } else if (character === "}" || character === "]") {
      depth -= 1;
      if (depth < 0) invalid("Tema", "JSON inválido: fechamento fora de ordem.");
    } else if (character === "," || character === ":") {
      separators += 1;
      if (separators > 1024) invalid("Tema", "a estrutura JSON contém propriedades ou itens demais.");
    }
  }
}

/** Parse and reconstruct a closed, bounded schema. AI output never becomes instructions. */
export function parseTheme(source: string): AuroraTheme {
  if (typeof source !== "string" || source.length > THEME_LIMITS.json || new TextEncoder().encode(source).byteLength > THEME_LIMITS.json) invalid("Tema", "o documento excede 34 MiB.");
  const json = withoutComments(source);
  checkJsonStructure(json);
  let value: unknown;
  try { value = JSON.parse(json); } catch { invalid("Tema", "JSON inválido. Cole somente o objeto JSON/JSONC, sem blocos de Markdown."); }
  const raw = closed(value, ["schemaVersion", "id", "name", "colors", "typography", "layout", "effects", "assets", "media", "icons", "decorations"], "Tema", ["window", "cursor"]);
  if (raw.schemaVersion !== 1) invalid("schemaVersion", "esta versão do Aurora aceita somente 1.");
  const id = identifier(raw.id, "id");
  if (id === "aurora-default") invalid("id", "aurora-default é reservado; escolha outro identificador.");
  const colors = closed(raw.colors, ["background", "panel", "text", "muted", "accent", "accentText", "border"], "colors");
  const typography = closed(raw.typography, ["font", "scale"], "typography");
  const layout = closed(raw.layout, ["sidebar", "density", "navOrder", "radius"], "layout", ["sidebarWidth", "statusSide", "sectionOrder", "decorationHeight"]);
  const effects = closed(raw.effects, ["background", "intensity", "speed", "glass"], "effects", ["transition", "transitionMs"]);
  const window = closed(raw.window === undefined ? {} : raw.window, [], "window", Object.keys(THEME_WINDOW_DEFAULTS));
  const cursor = closed(raw.cursor === undefined ? {} : raw.cursor, [], "cursor", Object.keys(THEME_CURSOR_DEFAULTS));
  const assetsObject = object(raw.assets, "assets");
  if (Object.keys(assetsObject).length > THEME_LIMITS.assetCount) invalid("assets", "use no máximo 4 recursos embutidos.");
  const assets: Record<string, ThemeAsset> = {};
  let totalBytes = 0;
  for (const [key, value] of Object.entries(assetsObject)) {
    identifier(key, "assets.id");
    const parsed = asset(value, `assets.${key}`);
    assets[key] = parsed.result;
    totalBytes += parsed.size;
    if (totalBytes > THEME_LIMITS.assets) invalid("assets", "os recursos somados excedem 24 MiB.");
  }
  function reference(value: unknown, path: string, png = false): string | null {
    if (value === null) return null;
    const key = identifier(value, path);
    if (!Object.hasOwn(assets, key)) invalid(path, "o recurso indicado não existe em assets.");
    if (png && assets[key].mime !== "image/png") invalid(path, "esta posição aceita somente PNG.");
    return key;
  }
  const media = closed(raw.media, ["background", "logo"], "media");
  const icons = closed(raw.icons, NAV_IDS, "icons");
  const parsedIcons = {} as Record<NavId, string>;
  for (const key of NAV_IDS) {
    const icon = icons[key];
    if (typeof icon === "string" && icon.startsWith("asset:")) { reference(icon.slice(6), `icons.${key}`, true); parsedIcons[key] = icon; }
    else parsedIcons[key] = choice(icon, THEME_ICON_NAMES, `icons.${key}`);
  }
  if (!Array.isArray(layout.navOrder) || layout.navOrder.length !== 4 || new Set(layout.navOrder).size !== 4) invalid("layout.navOrder", "inclua os quatro IDs de navegação exatamente uma vez.");
  const navOrder = layout.navOrder.map((entry, i) => choice(entry, NAV_IDS, `layout.navOrder[${i}]`));
  const sectionList = fallback(layout.sectionOrder, THEME_SECTION_IDS);
  if (!Array.isArray(sectionList) || sectionList.length !== 3 || new Set(sectionList).size !== 3) invalid("layout.sectionOrder", "inclua header, decorations e content exatamente uma vez.");
  const sectionOrder = sectionList.map((entry, i) => choice(entry, THEME_SECTION_IDS, `layout.sectionOrder[${i}]`));
  if (!Array.isArray(raw.decorations) || raw.decorations.length > 6) invalid("decorations", "use uma lista com até 6 elementos visuais.");
  const decorations = raw.decorations.map((entry, i) => {
    const path = `decorations[${i}]`;
    const decoration = closed(entry, ["type", "text", "asset", "align"], path, ["x", "y", "width", "height", "fontSize"]);
    const type = choice(decoration.type, ["text", "image"] as const, `${path}.type`);
    const image = reference(decoration.asset, `${path}.asset`, true);
    if (type === "image" && image === null) invalid(path, "uma decoração de imagem precisa de um recurso PNG.");
    if (type === "text" && image !== null) invalid(path, "uma decoração de texto usa asset: null.");
    return {
      type, text: text(decoration.text, `${path}.text`, 160, true), asset: image, align: choice(decoration.align, ["left", "center", "right"] as const, `${path}.align`),
      x: number(fallback(decoration.x, 50), 0, 100, `${path}.x`), y: number(fallback(decoration.y, 50), 0, 100, `${path}.y`),
      width: number(fallback(decoration.width, 30), 8, 100, `${path}.width`), height: number(fallback(decoration.height, 64), 24, 160, `${path}.height`),
      fontSize: number(fallback(decoration.fontSize, 20), 12, 56, `${path}.fontSize`),
    };
  });
  const parsedColors = { background: color(colors.background, "colors.background"), panel: color(colors.panel, "colors.panel"), text: color(colors.text, "colors.text"), muted: color(colors.muted, "colors.muted"), accent: color(colors.accent, "colors.accent"), accentText: color(colors.accentText, "colors.accentText"), border: color(colors.border, "colors.border") };
  contrast(parsedColors.text, parsedColors.panel, "colors.text/panel");
  contrast(parsedColors.accentText, parsedColors.accent, "colors.accentText/accent");
  const parsedWindow: AuroraTheme["window"] = {
    titlebarColor: color(fallback(window.titlebarColor, THEME_WINDOW_DEFAULTS.titlebarColor), "window.titlebarColor"),
    titlebarText: color(fallback(window.titlebarText, THEME_WINDOW_DEFAULTS.titlebarText), "window.titlebarText"),
    frameColor: color(fallback(window.frameColor, THEME_WINDOW_DEFAULTS.frameColor), "window.frameColor"),
    titlebarHeight: number(fallback(window.titlebarHeight, THEME_WINDOW_DEFAULTS.titlebarHeight), 36, 64, "window.titlebarHeight"),
    borderWidth: number(fallback(window.borderWidth, THEME_WINDOW_DEFAULTS.borderWidth), 0, 3, "window.borderWidth"),
    outerRadius: number(fallback(window.outerRadius, THEME_WINDOW_DEFAULTS.outerRadius), 0, 24, "window.outerRadius"),
    contentInset: number(fallback(window.contentInset, THEME_WINDOW_DEFAULTS.contentInset), 0, 24, "window.contentInset"),
  };
  contrast(parsedWindow.titlebarText, parsedWindow.titlebarColor, "window.titlebarText/titlebarColor");
  return {
    schemaVersion: 1, id, name: text(raw.name, "name", 60),
    colors: parsedColors,
    window: parsedWindow,
    cursor: {
      style: choice(fallback(cursor.style, THEME_CURSOR_DEFAULTS.style), ["system", "pixel", "ring", "crosshair"] as const, "cursor.style"),
      color: color(fallback(cursor.color, THEME_CURSOR_DEFAULTS.color), "cursor.color"),
      size: number(fallback(cursor.size, THEME_CURSOR_DEFAULTS.size), 16, 40, "cursor.size"),
      trail: choice(fallback(cursor.trail, THEME_CURSOR_DEFAULTS.trail), ["none", "sparkle"] as const, "cursor.trail"),
    },
    typography: { font: choice(typography.font, ["system", "rounded", "serif", "mono"] as const, "typography.font"), scale: number(typography.scale, 0.9, 1.15, "typography.scale") },
    layout: { sidebar: choice(layout.sidebar, ["left", "right"] as const, "layout.sidebar"), density: choice(layout.density, ["comfortable", "compact"] as const, "layout.density"), navOrder, radius: number(layout.radius, 0, 24, "layout.radius"),
      sidebarWidth: number(fallback(layout.sidebarWidth, 240), 180, 300, "layout.sidebarWidth"), statusSide: choice(fallback(layout.statusSide, "right"), ["right", "left"] as const, "layout.statusSide"), sectionOrder,
      decorationHeight: number(fallback(layout.decorationHeight, 160), 80, 400, "layout.decorationHeight") },
    effects: { background: choice(effects.background, ["none", "aurora", "water", "sakura"] as const, "effects.background"), intensity: number(effects.intensity, 0, 1, "effects.intensity"), speed: number(effects.speed, 0.25, 2, "effects.speed"), glass: number(effects.glass, 0, 16, "effects.glass"),
      transition: choice(fallback(effects.transition, "fade"), ["fade", "slide", "zoom", "none"] as const, "effects.transition"), transitionMs: number(fallback(effects.transitionMs, 300), 100, 700, "effects.transitionMs") },
    assets, media: { background: reference(media.background, "media.background"), logo: reference(media.logo, "media.logo", true) }, icons: parsedIcons, decorations,
  };
}

export const DEFAULT_THEME: AuroraTheme = {
  schemaVersion: 1, id: "aurora-default", name: "Aurora original",
  colors: { background: "#090714", panel: "#110d20", text: "#f2efff", muted: "#aea7bd", accent: "#c9a7ff", accentText: "#180b25", border: "#443557" },
  window: { ...THEME_WINDOW_DEFAULTS },
  cursor: { ...THEME_CURSOR_DEFAULTS },
  typography: { font: "system", scale: 1 },
  layout: { sidebar: "left", density: "comfortable", navOrder: [...NAV_IDS], radius: 12, sidebarWidth: 240, statusSide: "right", sectionOrder: [...THEME_SECTION_IDS], decorationHeight: 160 },
  effects: { background: "none", intensity: 0.5, speed: 1, glass: 0, transition: "fade", transitionMs: 300 }, assets: {}, media: { background: null, logo: null },
  icons: { instances: "default", discover: "default", appearance: "default", java: "default" }, decorations: [],
};

export const THEME_EXAMPLES: AuroraTheme[] = [
  { ...DEFAULT_THEME, id: "aurora-boreal", name: "Aurora boreal", colors: { background: "#080f19", panel: "#101d2b", text: "#edf8ff", muted: "#a5bbca", accent: "#83e9c4", accentText: "#09281e", border: "#2a4856" }, effects: { ...DEFAULT_THEME.effects, background: "aurora", intensity: 0.65, speed: 0.65, glass: 10 }, icons: { instances: "gamepad", discover: "sparkles", appearance: "star", java: "default" } },
  { ...DEFAULT_THEME, id: "jardim-sakura", name: "Jardim de cerejeiras", colors: { background: "#171322", panel: "#251d30", text: "#fff1f7", muted: "#c9b1c0", accent: "#f1adcd", accentText: "#361629", border: "#574050" }, typography: { font: "rounded", scale: 1 }, effects: { ...DEFAULT_THEME.effects, background: "sakura", intensity: 0.7, speed: 0.55, glass: 8 }, icons: { instances: "gamepad", discover: "leaf", appearance: "heart", java: "star" }, decorations: [{ type: "text", text: "Um novo mundo floresce a cada aventura.", asset: null, align: "center", x: 50, y: 50, width: 100, height: 64, fontSize: 20 }] },
  { ...DEFAULT_THEME, id: "lago-de-vidro", name: "Lago de vidro", colors: { background: "#071520", panel: "#102938", text: "#ecf9ff", muted: "#a2c3d2", accent: "#84d8f4", accentText: "#072b3a", border: "#315569" }, layout: { ...DEFAULT_THEME.layout, sidebar: "right", navOrder: ["instances", "appearance", "discover", "java"], radius: 18 }, effects: { ...DEFAULT_THEME.effects, background: "water", intensity: 0.6, speed: 0.45, glass: 14 }, icons: { instances: "gamepad", discover: "moon", appearance: "sparkles", java: "default" } },
];

export function themeVariables(theme: AuroraTheme): Record<string, string> {
  return {
    "--theme-background": theme.colors.background, "--theme-panel": theme.colors.panel,
    "--theme-text": theme.colors.text, "--theme-muted": theme.colors.muted,
    "--theme-accent": theme.colors.accent, "--theme-accent-text": theme.colors.accentText,
    "--theme-border": theme.colors.border, "--theme-font": FONT_FAMILIES[theme.typography.font],
    "--theme-scale": String(theme.typography.scale), "--theme-radius": `${theme.layout.radius}px`,
    "--theme-glass": `${theme.effects.glass}px`, "--theme-intensity": String(theme.effects.intensity),
    "--theme-speed": String(theme.effects.speed), "--theme-spacing": theme.layout.density === "compact" ? "0.85" : "1",
    "--theme-titlebar-color": theme.window.titlebarColor, "--theme-titlebar-text": theme.window.titlebarText,
    "--theme-frame-color": theme.window.frameColor, "--theme-titlebar-height": `${theme.window.titlebarHeight}px`,
    "--theme-border-width": `${theme.window.borderWidth}px`, "--theme-outer-radius": `${theme.window.outerRadius}px`,
    "--theme-content-inset": `${theme.window.contentInset}px`, "--theme-sidebar-width": `${theme.layout.sidebarWidth}px`,
    "--theme-decoration-height": `${theme.layout.decorationHeight}px`, "--theme-transition-duration": `${theme.effects.transitionMs}ms`,
    "--theme-cursor-color": theme.cursor.color, "--theme-cursor-size": `${theme.cursor.size}px`,
  };
}

const FIELD_COMMENTS: Record<string, string> = {
  schemaVersion: "Número inteiro fixo: 1. O documento descreve somente aparência.",
  id: "Texto único: 1–48 letras minúsculas, números e hífens. Não use aurora-default, constructor ou prototype.",
  name: "Nome exibido na biblioteca: texto com até 60 caracteres.",
  colors: "Cores sólidas #RRGGBB. Contraste mínimo 4.5:1 entre text/panel e accentText/accent.",
  "colors.background": "Cor #RRGGBB do fundo principal.",
  "colors.panel": "Cor #RRGGBB dos painéis e cartões.",
  "colors.text": "Cor #RRGGBB do texto principal; contraste mínimo 4.5:1 sobre panel.",
  "colors.muted": "Cor #RRGGBB de legendas e textos secundários.",
  "colors.accent": "Cor #RRGGBB de destaque e botões ativos.",
  "colors.accentText": "Cor #RRGGBB do texto sobre accent; contraste mínimo 4.5:1.",
  "colors.border": "Cor #RRGGBB de bordas e divisórias.",
  window: "Moldura visual da janela. As ações de minimizar, maximizar, fechar e restaurar continuam protegidas.",
  "window.titlebarColor": "Cor #RRGGBB do fundo da barra de título.",
  "window.titlebarText": "Cor #RRGGBB do texto da barra; contraste mínimo 4.5:1 sobre titlebarColor.",
  "window.frameColor": "Cor #RRGGBB da moldura exterior do conteúdo.",
  "window.titlebarHeight": "Altura da barra de título: número de 36 a 64 pixels.",
  "window.borderWidth": "Espessura da borda da moldura: número de 0 a 3 pixels.",
  "window.outerRadius": "Arredondamento da moldura: número de 0 a 24 pixels.",
  "window.contentInset": "Respiro entre a moldura e o conteúdo: número de 0 a 24 pixels.",
  cursor: "Mouse visual dentro do conteúdo do launcher; controles e áreas de segurança permanecem protegidos.",
  "cursor.style": "Estilo do cursor: system, pixel, ring ou crosshair.",
  "cursor.color": "Cor #RRGGBB do cursor personalizado.",
  "cursor.size": "Tamanho do cursor personalizado: número de 16 a 40 pixels.",
  "cursor.trail": "Rastro visual limitado: none ou sparkle; respeita movimento reduzido e janela oculta.",
  typography: "Tipografia local; nenhuma fonte é baixada.",
  "typography.font": "Fonte: system, rounded, serif ou mono.",
  "typography.scale": "Escala do texto: número de 0.9 a 1.15; 1 mantém o tamanho base.",
  layout: "Organização visual; IDs e ações do launcher continuam os mesmos.",
  "layout.sidebar": "Lado do menu de navegação: left ou right.",
  "layout.density": "Densidade dos espaços: comfortable ou compact.",
  "layout.navOrder": "Ordem do menu: instances, discover, appearance e java, exatamente uma vez cada.",
  "layout.radius": "Arredondamento dos cartões: número de 0 a 24 pixels.",
  "layout.sidebarWidth": "Largura do menu lateral: número de 180 a 300 pixels.",
  "layout.statusSide": "Lado dos painéis de status: right ou left.",
  "layout.sectionOrder": "Ordem das áreas: header, decorations e content, exatamente uma vez cada.",
  "layout.decorationHeight": "Altura da área reservada aos elementos visuais: número de 80 a 400 pixels.",
  effects: "Efeitos internos limitados; respeitam movimento reduzido e a janela oculta.",
  "effects.background": "Cena de fundo: none, aurora, water ou sakura.",
  "effects.intensity": "Intensidade do efeito: número de 0 a 1.",
  "effects.speed": "Velocidade do efeito: número de 0.25 a 2; 1 é a velocidade base.",
  "effects.glass": "Desfoque do vidro: número de 0 a 16 pixels.",
  "effects.transition": "Troca entre temas: fade, slide, zoom ou none.",
  "effects.transitionMs": "Duração da transição: número de 100 a 700 milissegundos.",
  assets: "Até 4 recursos locais embutidos; soma até 24 MiB. IDs seguem a regra de id. Documento até 34 MiB.",
  "assets.*": "Recurso embutido identificado por este nome. Nenhuma URL ou caminho local é aceito.",
  "assets.*.mime": "Tipo: image/png estático até 2 MiB/4096 px/8 milhões de pixels; image/gif só fundo até 24 MiB/1080p/240 frames/60 s/160 milhões de pixels acumulados; video/mp4 ou video/webm até 20 MiB/1080p/180 s.",
  "assets.*.data": "Conteúdo base64 puro, sem prefixo data:. Anexe o arquivo no Aurora; não invente este conteúdo.",
  media: "Referências aos IDs existentes em assets. Use null para desativar.",
  "media.background": "ID de PNG estático, GIF animado ou vídeo silencioso para o fundo, ou null.",
  "media.logo": "ID de PNG estático para o logo visual, ou null.",
  icons: "Ícones visuais das quatro seções; nenhuma ação é alterada.",
  "icons.*": "Ícone: default, star, leaf, moon, heart, gamepad, sparkles ou asset:ID de PNG existente.",
  decorations: "Lista com até 6 elementos somente visuais, posicionados dentro da área de decoração.",
  "decorations.*.type": "Tipo do elemento: text ou image.",
  "decorations.*.text": "Texto literal ou descrição da imagem: até 160 caracteres, sem caracteres de controle.",
  "decorations.*.asset": "ID de PNG existente para image; use null para text.",
  "decorations.*.align": "Alinhamento do texto dentro do elemento: left, center ou right.",
  "decorations.*.x": "Posição horizontal: 0–100% do espaço livre após descontar a largura; 0 à esquerda, 100 à direita.",
  "decorations.*.y": "Posição vertical: 0–100% do espaço livre após descontar a altura; 0 no topo, 100 embaixo.",
  "decorations.*.width": "Largura do elemento: número de 8 a 100% da área de decoração.",
  "decorations.*.height": "Altura do elemento: número de 24 a 160 pixels.",
  "decorations.*.fontSize": "Tamanho do texto do elemento: número de 12 a 56 pixels.",
};

/** Serialize even an intermediate editor state; saving/previewing performs validation separately. */
export function formatThemeCode(theme: AuroraTheme): string {
  function render(value: unknown, path: Array<string | number>, depth: number): string {
    const indent = "  ".repeat(depth);
    const childIndent = `${indent}  `;
    if (Array.isArray(value)) {
      if (value.length === 0 || value.every((entry) => entry === null || typeof entry !== "object")) return JSON.stringify(value);
      return `[\n${value.map((entry, i) => `${childIndent}${render(entry, [...path, i], depth + 1)}`).join(",\n")}\n${indent}]`;
    }
    if (value !== null && typeof value === "object") {
      const entries = Object.entries(value);
      if (entries.length === 0) return "{}";
      return `{\n${entries.map(([key, entry]) => {
        const next = [...path, key];
        const field = next.map((segment, i) => typeof segment === "number" || (i === 1 && ["assets", "icons"].includes(String(next[0]))) ? "*" : segment).join(".");
        const comment = FIELD_COMMENTS[field];
        return `${comment ? `${childIndent}// ${comment}\n` : ""}${childIndent}${JSON.stringify(key)}: ${render(entry, next, depth + 1)}`;
      }).join(",\n")}\n${indent}}`;
    }
    return JSON.stringify(value) ?? "null";
  }
  return `// Tema Aurora v1 — JSONC: comentários explicam campos, opções e unidades.\n// Edite os valores; o Aurora valida tudo antes da prévia de 10 segundos.\n${render(theme, [], 0)}\n`;
}

export function buildThemeAiPrompt(theme: AuroraTheme): string {
  return [
    "Crie ou ajuste um tema visual do Aurora Launcher conforme meu pedido ao final. Responda SOMENTE com um objeto JSON/JSONC válido, sem Markdown. O Aurora não executa código nem faz geração por IA: interpreta dados locais.",
    "Preserve os campos e recursos existentes. Não acrescente HTML, CSS, JavaScript, URLs, comandos, eventos, permissões ou funções. Comentários // e /* */ são permitidos fora das strings; não são executados. Não use vírgulas finais.",
    "",
    "Regras do schemaVersion 1:",
    "- id: 1–48 caracteres [a-z0-9][a-z0-9-]*, exceto aurora-default, constructor, prototype; name até 60 caracteres.",
    "- colors: background, panel, text, muted, accent, accentText, border; somente #RRGGBB. Contraste de text/panel e accentText/accent de pelo menos 4.5:1.",
    "- window: titlebarColor, titlebarText e frameColor #RRGGBB; contraste titlebarText/titlebarColor 4.5:1; titlebarHeight 36–64 px, borderWidth 0–3 px, outerRadius 0–24 px, contentInset 0–24 px. Os controles da janela preservam suas funções.",
    "- cursor: style system|pixel|ring|crosshair; color #RRGGBB; size 16–40 px; trail none|sparkle. É só aparência do mouse no conteúdo do launcher; não muda cliques, atalhos ou permissões. Rastro respeita movimento reduzido e a janela oculta.",
    "- typography: font system|rounded|serif|mono; scale 0.9–1.15. Fontes locais.",
    "- layout: sidebar left|right; density comfortable|compact; navOrder contém instances, discover, appearance, java uma vez cada; radius 0–24 px; sidebarWidth 180–300 px; statusSide right|left; sectionOrder contém header, decorations, content uma vez cada; decorationHeight 80–400 px.",
    "- effects: background none|aurora|water|sakura; intensity 0–1; speed 0.25–2; glass 0–16 px; transition fade|slide|zoom|none; transitionMs 100–700 ms. Somente efeitos internos, respeitando movimento reduzido e a janela oculta.",
    "- assets: até 4 IDs válidos, cada um com mime e data base64 puro, sem prefixo data:. PNG estático (sem APNG) image/png até 2 MiB/4096 px/8 milhões de pixels. GIF animado image/gif apenas no fundo, até 24 MiB/1920×1080/240 frames/160 milhões de pixels de canvas acumulados; até 60 segundos por ciclo somando o atraso de cada frame com mínimo de 40 ms. Somente looping padrão e comentários; sem extensões interativas ou externas. Vídeo silencioso video/mp4 ou video/webm até 20 MiB/1920×1080 (1080p)/180 segundos. Soma até 24 MiB; documento até 34 MiB. Preserve base64 existente; não invente recursos. Anexos faltantes são adicionados localmente no Aurora.",
    "- media: background ID de PNG/GIF/vídeo ou null; logo ID de PNG ou null. GIF não pode ser logo, ícone ou decoração. Sem caminhos nem URLs.",
    "- icons: instances, discover, appearance, java; cada valor default|star|leaf|moon|heart|gamepad|sparkles ou asset:ID de PNG existente.",
    "- decorations: até 6 objetos: type text|image; text até 160 caracteres literais; asset null para texto ou ID de PNG para imagem; align left|center|right; x e y 0–100% do espaço livre após descontar largura/altura; width 8–100%; height 24–160 px; fontSize 12–56 px. São elementos somente visuais.",
    "- Documentos antigos sem os novos campos recebem padrões; use os valores completos do exemplo abaixo. O usuário confirma dentro de uma prévia de 10 segundos. Autenticação, permissões, alertas de segurança e ações do launcher permanecem protegidos.",
    "",
    "Tema de referência comentado (nenhum dado de conta, credencial ou caminho local é acrescentado):",
    formatThemeCode(theme),
    "Meu pedido de aparência: ",
  ].join("\n");
}
