import { useEffect, useId, useRef, useState } from "react";
import { DEFAULT_THEME, formatThemeCode, inspectGif, parseTheme, THEME_EXAMPLES, type AuroraTheme, type ThemeAsset, type ThemeLibrary } from "./model";
import { ThemeVisualEditor } from "./ThemeVisualEditor";
import "./manager.css";

type Props = {
  library: ThemeLibrary;
  instances: Array<{ id: string; name: string }>;
  selectedInstanceId: string;
  busy: boolean;
  error: string;
  onSave: (json: string) => Promise<void>;
  onPreview: (id: string, instanceId: string | null) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
  onReset: () => Promise<void>;
  onAutomatic: (enabled: boolean) => Promise<void>;
  onUnbind: (instanceId: string) => Promise<void>;
};

const MIB = 1024 * 1024;
const MAX_JSON_BYTES = 34 * MIB;
const IMAGE_SOURCE_BYTES = 8 * MIB;
const VIDEO_BYTES = 20 * MIB;
const GIF_BYTES = 24 * MIB;
const EFFECT_NAMES: Record<AuroraTheme["effects"]["background"], string> = { none: "Sem efeito", aurora: "Aurora", water: "Água", sakura: "Cerejeira" };

function initialDocument() {
  return formatThemeCode({ ...DEFAULT_THEME, id: "meu-tema", name: "Meu tema" });
}

function readableError(error: unknown, fallback: string) {
  return error instanceof Error && error.message ? error.message : fallback;
}

function sameValue(left: unknown, right: unknown): boolean {
  if (left === right) return true;
  if (Array.isArray(left) && Array.isArray(right)) return left.length === right.length && left.every((value, index) => sameValue(value, right[index]));
  if (!left || !right || typeof left !== "object" || typeof right !== "object" || Array.isArray(left) || Array.isArray(right)) return false;
  const a = left as Record<string, unknown>, b = right as Record<string, unknown>;
  return Object.keys(a).length === Object.keys(b).length && Object.keys(a).every((key) => Object.hasOwn(b, key) && sameValue(a[key], b[key]));
}

function imageSize(bytes: Uint8Array): { width: number; height: number; mime: string } {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const text = (offset: number, length: number) => String.fromCharCode(...bytes.subarray(offset, offset + length));
  if (bytes.length >= 24 && [137, 80, 78, 71, 13, 10, 26, 10].every((value, index) => bytes[index] === value) && text(12, 4) === "IHDR") {
    return { width: view.getUint32(16), height: view.getUint32(20), mime: "image/png" };
  }
  if (bytes.length >= 30 && text(0, 4) === "RIFF" && text(8, 4) === "WEBP") {
    if (text(12, 4) === "VP8X") return { width: 1 + bytes[24] + (bytes[25] << 8) + (bytes[26] << 16), height: 1 + bytes[27] + (bytes[28] << 8) + (bytes[29] << 16), mime: "image/webp" };
    if (text(12, 4) === "VP8 " && bytes[23] === 0x9d && bytes[24] === 0x01 && bytes[25] === 0x2a) return { width: view.getUint16(26, true) & 0x3fff, height: view.getUint16(28, true) & 0x3fff, mime: "image/webp" };
    if (text(12, 4) === "VP8L" && bytes[20] === 0x2f) return { width: 1 + bytes[21] + ((bytes[22] & 0x3f) << 8), height: 1 + (bytes[22] >> 6) + (bytes[23] << 2) + ((bytes[24] & 0x0f) << 10), mime: "image/webp" };
  }
  if (bytes.length >= 4 && bytes[0] === 0xff && bytes[1] === 0xd8) {
    let offset = 2;
    while (offset + 3 < bytes.length) {
      if (bytes[offset++] !== 0xff) break;
      while (bytes[offset] === 0xff) offset += 1;
      const marker = bytes[offset++];
      if (marker === 0xd9 || marker === 0xda) break;
      if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd8)) continue;
      if (offset + 2 > bytes.length) break;
      const length = view.getUint16(offset);
      if (length < 2 || offset + length > bytes.length) break;
      if ([0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf].includes(marker) && length >= 8) return { width: view.getUint16(offset + 5), height: view.getUint16(offset + 3), mime: "image/jpeg" };
      offset += length;
    }
  }
  throw new Error("Escolha uma imagem PNG, JPEG ou WebP válida. SVG não é aceito.");
}

function asBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("Não foi possível ler o arquivo selecionado."));
    reader.onabort = () => reject(new Error("A leitura do arquivo foi cancelada."));
    reader.onload = () => typeof reader.result === "string" ? resolve(reader.result.slice(reader.result.indexOf(",") + 1)) : reject(new Error("Não foi possível ler o arquivo selecionado."));
    reader.readAsDataURL(blob);
  });
}

async function validateVideo(file: File) {
  const url = URL.createObjectURL(file);
  const video = document.createElement("video");
  video.muted = true;
  video.preload = "metadata";
  try {
    await new Promise<void>((resolve, reject) => {
      let settled = false;
      const finish = (error?: Error) => {
        if (settled) return;
        settled = true;
        window.clearTimeout(timer);
        if (error) reject(error); else resolve();
      };
      const timer = window.setTimeout(() => finish(new Error("O vídeo demorou para abrir. Use um arquivo menor ou outro formato.")), 8_000);
      video.onerror = () => finish(new Error("Este vídeo não pode ser aberto neste dispositivo. Use outro MP4 ou WebM."));
      video.onloadedmetadata = () => {
        if (!video.videoWidth || !video.videoHeight || video.videoWidth > 1920 || video.videoHeight > 1080 || video.videoWidth * video.videoHeight > 2_073_600) finish(new Error("Use um vídeo de até 1920 × 1080 pixels (Full HD)."));
        else if (!Number.isFinite(video.duration) || video.duration <= 0 || video.duration > 180) finish(new Error("Use um vídeo com duração de até 3 minutos."));
        else finish();
      };
      video.src = url;
    });
  } finally {
    video.onloadedmetadata = null;
    video.onerror = null;
    video.pause();
    video.removeAttribute("src");
    video.load();
    URL.revokeObjectURL(url);
  }
}

async function importAsset(file: File, target: "background" | "logo"): Promise<ThemeAsset> {
  if (!file.size || file.size > GIF_BYTES) throw new Error("Escolha um GIF de até 24 MiB, vídeo de até 20 MiB ou imagem estática.");
  const header = new Uint8Array(await file.slice(0, 32).arrayBuffer());
  const signature = String.fromCharCode(...header.subarray(0, 6));
  if (signature === "GIF87a" || signature === "GIF89a") {
    if (target !== "background") throw new Error("GIFs podem ser usados somente como fundo. Use uma imagem estática para logotipos e elementos.");
    inspectGif(new Uint8Array(await file.arrayBuffer()));
    return { mime: "image/gif", data: await asBase64(file) };
  }
  const isMp4 = header.length >= 12 && String.fromCharCode(...header.subarray(4, 8)) === "ftyp";
  const isWebm = header.length >= 4 && [0x1a, 0x45, 0xdf, 0xa3].every((value, index) => header[index] === value);
  if (isMp4 || isWebm) {
    if (target === "logo") throw new Error("O logotipo precisa ser uma imagem. Vídeos podem ser usados no fundo.");
    if (file.size > VIDEO_BYTES) throw new Error("O vídeo deve ter até 20 MiB.");
    await validateVideo(file);
    return { mime: isMp4 ? "video/mp4" : "video/webm", data: await asBase64(file) };
  }
  if (file.size > IMAGE_SOURCE_BYTES) throw new Error("A imagem de origem deve ter até 8 MiB.");
  const bytes = new Uint8Array(await file.arrayBuffer());
  const dimensions = imageSize(bytes);
  if (!dimensions.width || !dimensions.height || dimensions.width > 4096 || dimensions.height > 4096 || dimensions.width * dimensions.height > 8_000_000) throw new Error("Use uma imagem de até 4096 pixels por lado e 8 milhões de pixels no total.");
  const bitmap = await createImageBitmap(new Blob([bytes], { type: dimensions.mime }));
  try {
    if (bitmap.width > 4096 || bitmap.height > 4096 || bitmap.width * bitmap.height > 8_000_000) throw new Error("A imagem é grande demais. Reduza para até 8 milhões de pixels.");
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("Não foi possível preparar a imagem neste dispositivo.");
    context.drawImage(bitmap, 0, 0);
    const png = await new Promise<Blob>((resolve, reject) => canvas.toBlob((blob) => blob ? resolve(blob) : reject(new Error("Não foi possível converter a imagem para PNG.")), "image/png"));
    if (png.size > 2 * MIB) throw new Error("A imagem convertida ultrapassa 2 MiB. Escolha uma imagem menor.");
    return { mime: "image/png", data: await asBase64(png) };
  } finally {
    bitmap.close();
  }
}

function referencedElsewhere(theme: AuroraTheme, assetId: string) {
  return theme.media.background === assetId || theme.media.logo === assetId || Object.values(theme.icons).includes(`asset:${assetId}`) || theme.decorations.some((decoration) => decoration.asset === assetId);
}

function replaceMedia(theme: AuroraTheme, target: "background" | "logo", asset?: ThemeAsset): AuroraTheme {
  const next = { ...theme, assets: { ...theme.assets }, media: { ...theme.media, [target]: null } };
  const previous = theme.media[target];
  if (previous && !referencedElsewhere(next, previous)) delete next.assets[previous];
  if (asset) {
    let index = 1;
    let id = `local-${target}`;
    while (Object.hasOwn(next.assets, id)) id = `local-${target}-${index++}`;
    next.assets[id] = asset;
    next.media[target] = id;
  }
  return parseTheme(JSON.stringify(next));
}

export function ThemeManager({ library, instances, selectedInstanceId, busy, error, onSave, onPreview, onDelete, onReset, onAutomatic, onUnbind }: Props) {
  const id = useId();
  const [draft, setDraft] = useState(initialDocument);
  const [visual, setVisual] = useState<AuroraTheme>(() => ({ ...DEFAULT_THEME, id: "meu-tema", name: "Meu tema" }));
  const [codeStale, setCodeStale] = useState(false);
  const [mode, setMode] = useState<"visual" | "code">("visual");
  const [dirty, setDirty] = useState(false);
  const [replacement, setReplacement] = useState<{ theme: AuroraTheme; message: string } | null>(null);
  const [scope, setScope] = useState("");
  const [working, setWorking] = useState(false);
  const [localError, setLocalError] = useState("");
  const [message, setMessage] = useState("");
  const [deleteId, setDeleteId] = useState<string | null>(null);
  const editor = useRef<HTMLTextAreaElement>(null);
  const importPicker = useRef<HTMLInputElement>(null);
  const backgroundPicker = useRef<HTMLInputElement>(null);
  const logoPicker = useRef<HTMLInputElement>(null);
  const decorationPicker = useRef<HTMLInputElement>(null);
  const keepEditingButton = useRef<HTMLButtonElement>(null);
  const inFlight = useRef(false);
  const locked = busy || working;
  const selectedScope = instances.some((instance) => instance.id === scope) ? scope : "";
  const scopeName = instances.find((instance) => instance.id === selectedScope)?.name;
  const activeId = (library.automatic && library.bindings[selectedInstanceId]) || library.selectedThemeId;

  useEffect(() => { if (replacement) keepEditingButton.current?.focus(); }, [replacement]);

  function changeVisual(next: AuroraTheme) {
    setVisual(next);
    // Large embedded media is serialized only when code or an explicit action
    // needs it. Slider/pointer edits keep one authoritative visual document.
    if (mode === "code") { setDraft(formatThemeCode(next)); setCodeStale(false); }
    else setCodeStale(true);
    setDirty(true);
    setMessage("");
    setLocalError("");
  }

  function readTheme() {
    return parseTheme(mode === "visual" ? JSON.stringify(visual) : draft);
  }

  function applyDocument(theme: AuroraTheme, nextMessage: string) {
    setVisual(theme);
    setDraft(formatThemeCode(theme));
    setCodeStale(false);
    setDirty(!library.themes.some((item) => item.id === theme.id && sameValue(item, theme)));
    setReplacement(null);
    setLocalError("");
    setMessage(nextMessage);
    setDeleteId(null);
  }

  function requestDocument(theme: AuroraTheme, nextMessage: string) {
    if (dirty) setReplacement({ theme, message: nextMessage });
    else applyDocument(theme, nextMessage);
  }

  function switchMode(next: "visual" | "code") {
    if (locked || mode === next) return;
    setLocalError("");
    if (next === "visual") {
      try { setVisual(parseTheme(draft)); }
      catch (failure) { setLocalError(readableError(failure, "Corrija o código antes de abrir o editor visual.")); return; }
      setCodeStale(false);
    } else if (codeStale) {
      setDraft(formatThemeCode(visual));
      setCodeStale(false);
    }
    setMode(next);
  }

  async function perform(action: () => Promise<void>, fallback = "Não foi possível concluir a alteração.") {
    if (locked || inFlight.current) return;
    inFlight.current = true;
    setWorking(true);
    setLocalError("");
    setMessage("");
    try { await action(); } catch (failure) { setLocalError(readableError(failure, fallback)); }
    finally { inFlight.current = false; setWorking(false); }
  }

  function openDocument(theme: AuroraTheme) {
    if (locked) return;
    requestDocument(theme, `“${theme.name}” aberto no editor. Alterações só serão salvas ao clicar em Salvar.`);
  }

  async function saveDraft() {
    await perform(async () => {
      const theme = readTheme();
      await onSave(JSON.stringify(theme));
      setVisual(theme);
      setDraft(formatThemeCode(theme));
      setCodeStale(false);
      setDirty(false);
      setMessage(`“${theme.name}” salvo. Use a prévia de 10 segundos para aplicar.`);
    });
  }

  async function importDocument(file?: File) {
    if (!file) return;
    await perform(async () => {
      if (file.size > MAX_JSON_BYTES) throw new Error("O arquivo de tema deve ter até 34 MiB.");
      const theme = parseTheme(await file.text());
      requestDocument(theme, `“${theme.name}” importado no editor. Salve para incluir na biblioteca.`);
    }, "Não foi possível importar este arquivo de tema.");
  }

  async function attach(file: File | undefined, target: "background" | "logo") {
    if (!file) return;
    await perform(async () => {
      const theme = readTheme();
      const asset = await importAsset(file, target);
      changeVisual(replaceMedia(theme, target, asset));
      setMessage(`${target === "logo" ? "Logotipo incluído" : "Fundo incluído"} no código. Salve o tema para guardar a alteração.`);
    }, "Não foi possível preparar esta mídia. Escolha outro arquivo.");
  }

  async function removeMedia(target: "background" | "logo") {
    await perform(async () => {
      changeVisual(replaceMedia(readTheme(), target));
      setMessage(`${target === "logo" ? "Logotipo removido" : "Fundo removido"} do código. Salve para guardar a alteração.`);
    });
  }

  async function attachDecoration(file?: File) {
    if (!file) return;
    await perform(async () => {
      const theme = readTheme();
      if (theme.decorations.length >= 6) throw new Error("O tema já tem seis elementos. Remova um antes de adicionar outra imagem.");
      if (Object.keys(theme.assets).length >= 4) throw new Error("O tema já tem quatro arquivos. Remova um recurso antes de anexar outra imagem.");
      const asset = await importAsset(file, "logo");
      let assetId = "local-decoration";
      let index = 1;
      while (Object.hasOwn(theme.assets, assetId)) assetId = `local-decoration-${index++}`;
      const next = parseTheme(JSON.stringify({ ...theme, assets: { ...theme.assets, [assetId]: asset }, decorations: [...theme.decorations, { type: "image", text: "", asset: assetId, align: "center", x: 50, y: 50, width: 35, height: 80, fontSize: 20 }] }));
      changeVisual(next);
      setMode("visual");
      setMessage("Imagem adicionada ao painel decorativo. Arraste-a na prévia local para escolher a posição.");
    }, "Não foi possível preparar a imagem decorativa.");
  }

  async function exportDocument() {
    await perform(async () => {
      const theme = readTheme();
      const url = URL.createObjectURL(new Blob([formatThemeCode(theme)], { type: "application/json;charset=utf-8" }));
      const link = document.createElement("a");
      link.href = url;
      link.download = `${theme.id}.aurora-theme.jsonc`;
      document.body.append(link);
      link.click();
      link.remove();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
      setMessage("Exportação preparada com imagens, GIFs e vídeos incluídos no próprio tema.");
    }, "Não foi possível exportar o tema.");
  }

  async function testDraft() {
    await perform(async () => {
      const theme = readTheme();
      const saved = library.themes.find((item) => item.id === theme.id);
      if (!saved || !sameValue(saved, theme)) throw new Error("Salve as alterações na biblioteca antes de testar por 10 segundos.");
      await onPreview(theme.id, selectedScope || null);
    });
  }

  return <section className="theme-manager" aria-labelledby={`${id}-title`} aria-busy={locked}>
    <header className="theme-manager-heading">
      <div><p className="eyebrow">SEU AURORA</p><h2 id={`${id}-title`}>Personalização</h2><p>Crie seu ambiente, guarde seus temas e escolha uma aparência para cada modpack.</p></div>
      <button className="secondary compact" disabled={locked} onClick={() => { void perform(onReset); }} type="button">Restaurar padrão</button>
    </header>

    {!library.persistent && <p className="theme-manager-info" role="status">Biblioteca temporária nesta sessão. Outra janela pode estar usando os temas salvos; exporte seu código para guardá-lo.</p>}
    {library.recovered && <p className="theme-manager-info" role="status">O Aurora voltou ao visual padrão após uma interrupção. Seus temas continuam salvos; teste novamente antes de ativá-los.</p>}

    <div className="theme-manager-library">
      <div className="theme-manager-section-heading"><div><h3>Meus temas</h3><p>{library.themes.length ? `${library.themes.length} ${library.themes.length === 1 ? "tema salvo" : "temas salvos"}` : "Salve um tema para começar sua coleção."}</p></div><span className="theme-manager-badge">Somente visual</span></div>
      <div className="theme-manager-scope">
        <label htmlFor={`${id}-scope`}>Onde aplicar a próxima prévia?</label>
        <select id={`${id}-scope`} value={selectedScope} disabled={locked} onChange={(event) => setScope(event.target.value)}>
          <option value="">Launcher inteiro</option>
          {instances.map((instance) => <option key={instance.id} value={instance.id}>{instance.name}</option>)}
        </select>
        <p>{scopeName ? `Após confirmar, este tema aparecerá ao selecionar “${scopeName}”.` : "Após confirmar, este será o tema geral do launcher."}</p>
      </div>
      <label className="theme-manager-automatic"><input type="checkbox" checked={library.automatic} disabled={locked} onChange={(event) => { const enabled = event.target.checked; void perform(() => onAutomatic(enabled)); }} /><span><strong>Trocar tema ao selecionar um modpack</strong><small>Usa as associações que você confirmou. Desativar preserva os vínculos salvos.</small></span></label>
      {Object.keys(library.bindings).length > 0 && <details className="theme-manager-bindings"><summary>Associações salvas ({Object.keys(library.bindings).length})</summary><ul>{Object.entries(library.bindings).map(([instanceId, themeId]) => <li key={instanceId}><span><strong>{instances.find((instance) => instance.id === instanceId)?.name ?? instanceId}</strong><small>{library.themes.find((theme) => theme.id === themeId)?.name ?? "Tema indisponível"}</small></span><button className="secondary compact" disabled={locked} onClick={() => { void perform(async () => { await onUnbind(instanceId); setMessage("Associação removida. Este modpack usará o tema geral."); }); }} type="button" aria-label={`Remover associação de ${instances.find((instance) => instance.id === instanceId)?.name ?? instanceId}`}>Remover vínculo</button></li>)}</ul></details>}
      <div className="theme-manager-cards">
        {library.themes.map((theme) => {
          const boundInstances = instances.filter((instance) => library.bindings[instance.id] === theme.id);
          return <article className={`theme-manager-card${activeId === theme.id ? " is-active" : ""}`} key={theme.id}>
            <div className="theme-manager-swatches" aria-hidden="true"><i style={{ backgroundColor: theme.colors.background }} /><i style={{ backgroundColor: theme.colors.panel }} /><i style={{ backgroundColor: theme.colors.accent }} /><i style={{ backgroundColor: theme.colors.text }} /></div>
            <div className="theme-manager-card-copy"><div><h4>{theme.name}</h4>{activeId === theme.id && <span className="theme-manager-active">Em uso</span>}</div><p>{EFFECT_NAMES[theme.effects.background]}{theme.media.background && theme.assets[theme.media.background]?.mime.startsWith("video/") ? " · Vídeo" : theme.media.background && theme.assets[theme.media.background]?.mime === "image/gif" ? " · GIF" : ""} · Menu à {theme.layout.sidebar === "left" ? "esquerda" : "direita"}</p>{boundInstances.length > 0 && <small title={boundInstances.map((instance) => instance.name).join(", ")}>{boundInstances.length === 1 ? `Vinculado a ${boundInstances[0].name}` : `Vinculado a ${boundInstances.length} modpacks`}</small>}</div>
            {deleteId === theme.id ? <div className="theme-manager-delete" role="group" aria-label={`Remover ${theme.name}`}><p>Remover este tema e seus vínculos?</p><button className="danger compact" disabled={locked} onClick={() => { void perform(async () => { await onDelete(theme.id); setDeleteId(null); setMessage("Tema removido da biblioteca."); }); }} type="button">Remover</button><button className="secondary compact" disabled={locked} onClick={() => setDeleteId(null)} type="button">Cancelar</button></div> : <div className="theme-manager-card-actions"><button className="compact" disabled={locked} onClick={() => { void perform(() => onPreview(theme.id, selectedScope || null)); }} type="button">Prévia de 10 s</button><button className="secondary compact" disabled={locked} onClick={() => openDocument(theme)} type="button">Editar</button><button aria-label={`Remover tema ${theme.name}`} className="theme-manager-remove" disabled={locked} onClick={() => setDeleteId(theme.id)} type="button">×</button></div>}
          </article>;
        })}
        {library.themes.length === 0 && <div className="theme-manager-empty"><span aria-hidden="true">✦</span><strong>Um launcher com a sua cara</strong><p>Experimente um modelo abaixo ou cole o código do seu tema.</p></div>}
      </div>
    </div>

    <div className="theme-manager-editor">
      <div className="theme-manager-section-heading"><div><h3>Monte seu tema</h3><p>Organize visualmente ou edite o código comentado. Os dois modos trabalham no mesmo tema.</p></div></div>
      <div className="theme-manager-examples" role="group" aria-label="Modelos de tema"><span>Começar com</span><button className="secondary compact" disabled={locked} onClick={() => openDocument({ ...DEFAULT_THEME, id: "meu-tema", name: "Meu tema" })} type="button">Novo tema</button>{THEME_EXAMPLES.map((theme) => <button className="secondary compact" disabled={locked} key={theme.id} onClick={() => openDocument(theme)} type="button">{theme.name}</button>)}</div>
      {replacement && <div className="theme-manager-replace" role="alertdialog" aria-labelledby={`${id}-replace-title`}><strong id={`${id}-replace-title`}>Você tem alterações não salvas</strong><p>Abrir “{replacement.theme.name}” substituirá o rascunho atual.</p><div><button className="secondary compact" ref={keepEditingButton} disabled={locked} type="button" onClick={() => setReplacement(null)}>Continuar editando</button><button className="danger compact" disabled={locked} type="button" onClick={() => applyDocument(replacement.theme, replacement.message)}>Descartar rascunho e abrir</button></div></div>}
      <div className="theme-manager-mode" role="tablist" aria-label="Modo de edição"><button id={`${id}-visual-tab`} type="button" role="tab" aria-selected={mode === "visual"} aria-controls={`${id}-visual-panel`} disabled={locked} className={mode === "visual" ? "is-selected" : ""} onClick={() => switchMode("visual")}>Editor visual</button><button id={`${id}-code-tab`} type="button" role="tab" aria-selected={mode === "code"} aria-controls={`${id}-code-panel`} disabled={locked} className={mode === "code" ? "is-selected" : ""} onClick={() => switchMode("code")}>Código comentado</button><span>{dirty ? "Alterações não salvas" : "Rascunho sincronizado"}</span></div>
      {localError && <p className="theme-manager-error" role="alert">{localError}</p>}
      {mode === "visual" ? <div id={`${id}-visual-panel`} role="tabpanel" aria-labelledby={`${id}-visual-tab`}><ThemeVisualEditor theme={visual} disabled={locked} onChange={changeVisual} onImportImage={() => decorationPicker.current?.click()} /></div> : <div id={`${id}-code-panel`} role="tabpanel" aria-labelledby={`${id}-code-tab`} className="theme-manager-code-panel"><label className="theme-manager-editor-label" htmlFor={`${id}-code`}>Cole ou edite o código completo</label><textarea ref={editor} id={`${id}-code`} aria-describedby={`${id}-code-hint`} autoCapitalize="off" autoComplete="off" spellCheck={false} maxLength={MAX_JSON_BYTES} disabled={locked} value={draft} onChange={(event) => { setDraft(event.target.value); setCodeStale(false); setDirty(true); setMessage(""); setLocalError(""); }} /><p className="theme-manager-hint" id={`${id}-code-hint`}>Cada comentário // explica o campo logo abaixo. Você pode manter os comentários; o Aurora aceita JSON e JSON com comentários. Corrija erros antes de voltar ao modo visual.</p></div>}
      <div className="theme-manager-attachments" role="group" aria-label="Mídia do tema"><div><strong>Inclua seus arquivos</strong><span>Imagens estáticas viram PNG. GIFs: até 24 MiB e 60 segundos. Vídeos MP4/WebM: até 20 MiB e 3 minutos. Animações até Full HD, sempre sem som.</span></div><button className="secondary compact" disabled={locked} onClick={() => backgroundPicker.current?.click()} type="button">Adicionar fundo, GIF ou vídeo</button><button className="secondary compact" disabled={locked} onClick={() => logoPicker.current?.click()} type="button">Adicionar logotipo</button><button className="secondary compact" disabled={locked} onClick={() => decorationPicker.current?.click()} type="button">Adicionar imagem decorativa</button><details><summary>Remover mídia</summary><button className="text-button" disabled={locked} onClick={() => { void removeMedia("background"); }} type="button">Remover fundo</button><button className="text-button" disabled={locked} onClick={() => { void removeMedia("logo"); }} type="button">Remover logotipo</button></details></div>
      <div className="theme-manager-editor-actions"><div><button className="secondary compact" disabled={locked} onClick={() => importPicker.current?.click()} type="button">Importar arquivo</button><button className="secondary compact" disabled={locked} onClick={() => { void exportDocument(); }} type="button">Exportar tema</button><button className="secondary compact" disabled={locked} onClick={() => { void perform(async () => { readTheme(); setMessage("Código válido. Salve na biblioteca para testar a aparência."); }); }} type="button">Verificar código</button></div><div className="theme-manager-save-actions"><button disabled={locked} onClick={() => { void saveDraft(); }} type="button">{working ? "Preparando…" : "Salvar na biblioteca"}</button><button className="secondary" disabled={locked || dirty || !library.themes.some((item) => item.id === visual.id)} onClick={() => { void testDraft(); }} type="button">Testar por 10 s</button></div></div>
      <input ref={importPicker} className="theme-manager-file" type="file" accept=".json,.jsonc,application/json" aria-label="Importar arquivo JSON de tema" disabled={locked} onChange={(event) => { const file = event.target.files?.[0]; event.target.value = ""; void importDocument(file); }} />
      <input ref={backgroundPicker} className="theme-manager-file" type="file" accept="image/png,image/jpeg,image/webp,image/gif,video/mp4,video/webm" aria-label="Selecionar imagem, GIF ou vídeo de fundo" disabled={locked} onChange={(event) => { const file = event.target.files?.[0]; event.target.value = ""; void attach(file, "background"); }} />
      <input ref={decorationPicker} className="theme-manager-file" type="file" accept="image/png,image/jpeg,image/webp" aria-label="Selecionar imagem decorativa" disabled={locked} onChange={(event) => { const file = event.target.files?.[0]; event.target.value = ""; void attachDecoration(file); }} />
      <input ref={logoPicker} className="theme-manager-file" type="file" accept="image/png,image/jpeg,image/webp" aria-label="Selecionar logotipo" disabled={locked} onChange={(event) => { const file = event.target.files?.[0]; event.target.value = ""; void attach(file, "logo"); }} />
    </div>
    {error && error !== localError && <p className="theme-manager-error" role="alert">{error}</p>}
    {message && <p className="theme-manager-status" role="status">{message}</p>}
    <p className="theme-manager-footer">Prévias sem confirmação e falhas de execução restauram o padrão. Os temas salvos permanecem na biblioteca.</p>
  </section>;
}
