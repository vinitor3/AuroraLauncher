/** Theme documents are untrusted data. Never turn their contents into markup or code. */
export type NavId = "instances" | "discover" | "appearance" | "java";
export type ThemeAsset = { mime: "image/png" | "video/mp4" | "video/webm"; data: string };
export type AuroraTheme = {
  schemaVersion: 1;
  id: string;
  name: string;
  colors: { background: string; panel: string; text: string; muted: string; accent: string; accentText: string; border: string };
  typography: { font: "system" | "rounded" | "serif" | "mono"; scale: number };
  layout: { sidebar: "left" | "right"; density: "comfortable" | "compact"; navOrder: NavId[]; radius: number };
  effects: { background: "none" | "aurora" | "water" | "sakura"; intensity: number; speed: number; glass: number };
  assets: Record<string, ThemeAsset>;
  media: { background: string | null; logo: string | null };
  icons: Record<NavId, string>;
  decorations: Array<{ type: "text" | "image"; text: string; asset: string | null; align: "left" | "center" | "right" }>;
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
export const THEME_LIMITS = { json: 34 * MIB, png: 2 * MIB, video: 20 * MIB, assets: 24 * MIB, assetCount: 4 } as const;
export const NAV_IDS: readonly NavId[] = ["instances", "discover", "appearance", "java"];
export const THEME_ICON_NAMES = ["default", "star", "leaf", "moon", "heart", "gamepad", "sparkles"] as const;
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
function closed(value: unknown, keys: readonly string[], path: string): Record<string, unknown> {
  const result = object(value, path);
  for (const key of Object.keys(result)) if (!keys.includes(key)) invalid(path, `campo não permitido: ${key.slice(0, 60)}.`);
  for (const key of keys) if (!Object.hasOwn(result, key)) invalid(path, `campo obrigatório: ${key}.`);
  return result;
}
function text(value: unknown, path: string, max: number, allowEmpty = false): string {
  if (typeof value !== "string" || (!allowEmpty && !value.trim()) || Array.from(value).length > max || /[\u0000-\u001f\u007f-\u009f]/.test(value)) {
    invalid(path, `use texto ${allowEmpty ? "de até" : "não vazio de até"} ${max} caracteres, sem controles.`);
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
function asset(value: unknown, path: string): { result: ThemeAsset; size: number } {
  const raw = closed(value, ["mime", "data"], path);
  const mime = choice(raw.mime, ["image/png", "video/mp4", "video/webm"] as const, `${path}.mime`);
  const encoded = raw.data;
  const limit = mime === "image/png" ? THEME_LIMITS.png : THEME_LIMITS.video;
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
  } else if (mime === "video/mp4") {
    const brands = ["isom", "iso2", "iso3", "iso4", "iso5", "iso6", "mp41", "mp42", "avc1", "M4V "];
    if (header.length < 16 || String.fromCharCode(...header.slice(4, 8)) !== "ftyp" || u32(header, 0) < 16 || u32(header, 0) > size || u32(header, 0) % 4 !== 0 || !brands.includes(String.fromCharCode(...header.slice(8, 12)))) invalid(path, "assinatura ou marca MP4 inválida.");
  } else if (!validWebm(header)) invalid(path, "assinatura ou DocType WebM inválido.");
  return { result: { mime, data: encoded }, size };
}

/** Bound structure before JSON.parse can allocate a large tree. Embedded bytes are strings. */
function checkJsonStructure(source: string): void {
  let depth = 0, containers = 0, separators = 0;
  for (let index = 0; index < source.length; index += 1) {
    const character = source[index];
    if (character === '"') {
      let quote = source.indexOf('"', index + 1);
      while (quote >= 0) {
        let backslashes = 0;
        for (let before = quote - 1; before >= 0 && source[before] === "\\"; before -= 1) backslashes += 1;
        if (backslashes % 2 === 0) break;
        quote = source.indexOf('"', quote + 1);
      }
      if (quote < 0) invalid("Tema", "JSON inválido: texto sem fechamento.");
      index = quote;
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
  checkJsonStructure(source);
  let value: unknown;
  try { value = JSON.parse(source); } catch { invalid("Tema", "JSON inválido. Cole somente o objeto JSON, sem blocos de Markdown."); }
  const raw = closed(value, ["schemaVersion", "id", "name", "colors", "typography", "layout", "effects", "assets", "media", "icons", "decorations"], "Tema");
  if (raw.schemaVersion !== 1) invalid("schemaVersion", "esta versão do Aurora aceita somente 1.");
  const id = identifier(raw.id, "id");
  if (id === "aurora-default") invalid("id", "aurora-default é reservado; escolha outro identificador.");
  const colors = closed(raw.colors, ["background", "panel", "text", "muted", "accent", "accentText", "border"], "colors");
  const typography = closed(raw.typography, ["font", "scale"], "typography");
  const layout = closed(raw.layout, ["sidebar", "density", "navOrder", "radius"], "layout");
  const effects = closed(raw.effects, ["background", "intensity", "speed", "glass"], "effects");
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
  if (!Array.isArray(raw.decorations) || raw.decorations.length > 6) invalid("decorations", "use uma lista com até 6 elementos visuais.");
  const decorations = raw.decorations.map((entry, i) => {
    const path = `decorations[${i}]`;
    const decoration = closed(entry, ["type", "text", "asset", "align"], path);
    const type = choice(decoration.type, ["text", "image"] as const, `${path}.type`);
    const image = reference(decoration.asset, `${path}.asset`, true);
    if (type === "image" && image === null) invalid(path, "uma decoração de imagem precisa de um recurso PNG.");
    if (type === "text" && image !== null) invalid(path, "uma decoração de texto usa asset: null.");
    return { type, text: text(decoration.text, `${path}.text`, 160, true), asset: image, align: choice(decoration.align, ["left", "center", "right"] as const, `${path}.align`) };
  });
  const parsedColors = { background: color(colors.background, "colors.background"), panel: color(colors.panel, "colors.panel"), text: color(colors.text, "colors.text"), muted: color(colors.muted, "colors.muted"), accent: color(colors.accent, "colors.accent"), accentText: color(colors.accentText, "colors.accentText"), border: color(colors.border, "colors.border") };
  contrast(parsedColors.text, parsedColors.panel, "colors.text/panel");
  contrast(parsedColors.accentText, parsedColors.accent, "colors.accentText/accent");
  return {
    schemaVersion: 1, id, name: text(raw.name, "name", 60),
    colors: parsedColors,
    typography: { font: choice(typography.font, ["system", "rounded", "serif", "mono"] as const, "typography.font"), scale: number(typography.scale, 0.9, 1.15, "typography.scale") },
    layout: { sidebar: choice(layout.sidebar, ["left", "right"] as const, "layout.sidebar"), density: choice(layout.density, ["comfortable", "compact"] as const, "layout.density"), navOrder, radius: number(layout.radius, 0, 24, "layout.radius") },
    effects: { background: choice(effects.background, ["none", "aurora", "water", "sakura"] as const, "effects.background"), intensity: number(effects.intensity, 0, 1, "effects.intensity"), speed: number(effects.speed, 0.25, 2, "effects.speed"), glass: number(effects.glass, 0, 16, "effects.glass") },
    assets, media: { background: reference(media.background, "media.background"), logo: reference(media.logo, "media.logo", true) }, icons: parsedIcons, decorations,
  };
}

export const DEFAULT_THEME: AuroraTheme = {
  schemaVersion: 1, id: "aurora-default", name: "Aurora original",
  colors: { background: "#090714", panel: "#110d20", text: "#f2efff", muted: "#aea7bd", accent: "#c9a7ff", accentText: "#180b25", border: "#443557" },
  typography: { font: "system", scale: 1 },
  layout: { sidebar: "left", density: "comfortable", navOrder: [...NAV_IDS], radius: 12 },
  effects: { background: "none", intensity: 0.5, speed: 1, glass: 0 }, assets: {}, media: { background: null, logo: null },
  icons: { instances: "default", discover: "default", appearance: "default", java: "default" }, decorations: [],
};

export const THEME_EXAMPLES: AuroraTheme[] = [
  { ...DEFAULT_THEME, id: "aurora-boreal", name: "Aurora boreal", colors: { background: "#080f19", panel: "#101d2b", text: "#edf8ff", muted: "#a5bbca", accent: "#83e9c4", accentText: "#09281e", border: "#2a4856" }, effects: { background: "aurora", intensity: 0.65, speed: 0.65, glass: 10 }, icons: { instances: "gamepad", discover: "sparkles", appearance: "star", java: "default" } },
  { ...DEFAULT_THEME, id: "jardim-sakura", name: "Jardim de cerejeiras", colors: { background: "#171322", panel: "#251d30", text: "#fff1f7", muted: "#c9b1c0", accent: "#f1adcd", accentText: "#361629", border: "#574050" }, typography: { font: "rounded", scale: 1 }, effects: { background: "sakura", intensity: 0.7, speed: 0.55, glass: 8 }, icons: { instances: "gamepad", discover: "leaf", appearance: "heart", java: "star" }, decorations: [{ type: "text", text: "Um novo mundo floresce a cada aventura.", asset: null, align: "center" }] },
  { ...DEFAULT_THEME, id: "lago-de-vidro", name: "Lago de vidro", colors: { background: "#071520", panel: "#102938", text: "#ecf9ff", muted: "#a2c3d2", accent: "#84d8f4", accentText: "#072b3a", border: "#315569" }, layout: { sidebar: "right", density: "comfortable", navOrder: ["instances", "appearance", "discover", "java"], radius: 18 }, effects: { background: "water", intensity: 0.6, speed: 0.45, glass: 14 }, icons: { instances: "gamepad", discover: "moon", appearance: "sparkles", java: "default" } },
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
  };
}

export function buildThemeAiPrompt(theme: AuroraTheme): string {
  return `Crie ou ajuste um tema visual do Aurora Launcher conforme meu pedido ao final. Responda SOMENTE com um objeto JSON válido, sem Markdown. O Aurora não executa código nem faz geração por IA: interpreta o formato fechado abaixo. Preserve todos os campos obrigatórios e não acrescente campos. Não inclua HTML, CSS, JavaScript, URLs, comandos, eventos, permissões nem funções novas. Textos são exibidos literalmente.\n\nRegras do schemaVersion 1:\n- id: 1–48 caracteres [a-z0-9][a-z0-9-]*, exceto aurora-default, constructor, prototype. name: texto até 60 caracteres.\n- colors: background, panel, text, muted, accent, accentText, border; somente #RRGGBB. Contraste de text/panel e accentText/accent de pelo menos 4.5:1.\n- typography: font = system|rounded|serif|mono, scale entre 0.9 e 1.15. Fontes locais.\n- layout: sidebar = left|right; density = comfortable|compact; navOrder contém instances, discover, appearance, java exatamente uma vez; radius 0–24.\n- effects: background = none|aurora|water|sakura; intensity 0–1; speed 0.25–2; glass 0–16. Os efeitos internos são suaves e respeitam movimento reduzido. Não invente novos efeitos.\n- assets: objeto com até 4 IDs válidos; cada recurso tem apenas mime e data. data é base64 puro, sem prefixo data:. PNG estático (sem APNG): image/png, até 2 MiB, dimensão até 4096 e 8 milhões de pixels. Vídeo silencioso: video/mp4 ou video/webm, até 20 MiB, resolução máxima 1920×1080 (1080p), duração definida de até 180 segundos. Soma até 24 MiB; documento até 34 MiB. Não invente base64: preserve recursos já anexados e seus IDs; se faltarem, use assets vazio e peça que eu anexe no Aurora.\n- media: background = ID de PNG/vídeo ou null; logo = ID de PNG ou null. Sem caminhos nem URLs.\n- icons: objeto com instances, discover, appearance, java. Cada valor = default|star|leaf|moon|heart|gamepad|sparkles ou asset:ID de PNG existente.\n- decorations: até 6 objetos com type, text, asset, align. type = text|image; text até 160 caracteres; asset null para texto ou ID de PNG para imagem; align = left|center|right. Elementos são somente visuais.\n- O usuário confirma dentro de uma prévia de 10 segundos antes de salvar a escolha. Temas nunca alteram autenticação, permissões ou ações do launcher.\n\nTema de referência (nenhum dado de conta, credencial ou caminho local):\n${JSON.stringify(theme, null, 2)}\n\nMeu pedido de aparência: `;
}
