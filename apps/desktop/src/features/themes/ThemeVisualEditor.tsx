import { useEffect, useId, useRef, useState, type CSSProperties, type DragEvent, type PointerEvent } from "react";
import { type AuroraTheme, type NavId, THEME_ICON_NAMES } from "./model";
import "./visual-editor.css";

type Props = { theme: AuroraTheme; disabled: boolean; onChange: (theme: AuroraTheme) => void; onImportImage: () => void };
type SectionId = AuroraTheme["layout"]["sectionOrder"][number];
type Decoration = AuroraTheme["decorations"][number];
type DragItem = { kind: "nav"; id: NavId } | { kind: "section"; id: SectionId } | { kind: "sidebar" | "status" };
type DragPosition = { index: number; x: number; y: number };
const NAV_NAMES: Record<NavId, string> = { instances: "Modpacks", discover: "Descobrir", appearance: "Guarda-roupa", java: "Java" };
const SECTION_NAMES: Record<SectionId, string> = { header: "Cabeçalho", decorations: "Painel decorativo", content: "Conteúdo" };
const ICONS: Record<string, string> = { default: "◈", star: "★", leaf: "❧", moon: "☾", heart: "♥", gamepad: "✚", sparkles: "✦" };
const ICON_NAMES: Record<string, string> = { default: "Padrão", star: "Estrela", leaf: "Folha", moon: "Lua", heart: "Coração", gamepad: "Controle", sparkles: "Brilho" };
const FONT_FAMILIES = { system: "system-ui, sans-serif", rounded: '"Arial Rounded MT Bold", "Trebuchet MS", sans-serif', serif: 'Georgia, "Times New Roman", serif', mono: "Consolas, monospace" };
const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

function moveItem<T extends string>(items: T[], source: T, target: T): T[] {
  const from = items.indexOf(source);
  const to = items.indexOf(target);
  if (from < 0 || to < 0 || from === to) return items;
  const result = [...items];
  result.splice(from, 1);
  result.splice(to, 0, source);
  return result;
}

function ColorField({ label, value, disabled, onChange }: { label: string; value: string; disabled: boolean; onChange: (value: string) => void }) {
  const id = useId();
  return <label className="tv-color" htmlFor={id}><span>{label}</span><div><input type="color" value={/^#[0-9a-f]{6}$/i.test(value) ? value : "#000000"} disabled={disabled} onChange={(event) => onChange(event.target.value)} aria-label={`Escolher cor: ${label}`} /><input id={id} type="text" value={value} maxLength={7} disabled={disabled} spellCheck={false} onChange={(event) => onChange(event.target.value)} /></div></label>;
}

function Slider({ label, value, min, max, step = 1, unit = "", disabled, onChange }: { label: string; value: number; min: number; max: number; step?: number; unit?: string; disabled: boolean; onChange: (value: number) => void }) {
  const id = useId();
  return <label className="tv-slider" htmlFor={id}><span>{label}<output>{Number.isInteger(value) ? value : Number(value.toFixed(2))}{unit}</output></span><input id={id} type="range" min={min} max={max} step={step} value={value} disabled={disabled} onChange={(event) => onChange(Number(event.target.value))} /></label>;
}

function usePngResources(assets: AuroraTheme["assets"]) {
  const [urls, setUrls] = useState<Record<string, string>>({});
  useEffect(() => {
    const next: Record<string, string> = {};
    for (const [id, asset] of Object.entries(assets)) {
      if (asset.mime !== "image/png") continue;
      const bytes = Uint8Array.from(atob(asset.data), (character) => character.charCodeAt(0));
      next[id] = URL.createObjectURL(new Blob([bytes], { type: "image/png" }));
    }
    setUrls(next);
    return () => Object.values(next).forEach((url) => URL.revokeObjectURL(url));
  }, [assets]);
  return urls;
}

export function ThemeVisualEditor({ theme, disabled, onChange, onImportImage }: Props) {
  const editorId = useId();
  const [tab, setTab] = useState<"layout" | "colors" | "window" | "motion" | "cursor" | "elements">("layout");
  const [selected, setSelected] = useState(0);
  const [dragging, setDragging] = useState<DragItem | null>(null);
  const [dragPosition, setDragPosition] = useState<DragPosition | null>(null);
  const [dragNotice, setDragNotice] = useState("");
  const pointer = useRef<{ pointerId: number; index: number; startX: number; startY: number; x: number; y: number; freeWidth: number; freeHeight: number } | null>(null);
  const area = useRef<HTMLDivElement>(null);
  const urls = usePngResources(theme.assets);
  const selectedIndex = Math.min(selected, Math.max(0, theme.decorations.length - 1));
  const decoration = theme.decorations[selectedIndex];
  const pngIds = Object.keys(theme.assets).filter((id) => theme.assets[id].mime === "image/png");
  const backgroundAsset = theme.media.background ? theme.assets[theme.media.background] : null;
  const patchLayout = (patch: Partial<AuroraTheme["layout"]>) => onChange({ ...theme, layout: { ...theme.layout, ...patch } });
  const patchWindow = (patch: Partial<AuroraTheme["window"]>) => onChange({ ...theme, window: { ...theme.window, ...patch } });
  const patchEffect = (patch: Partial<AuroraTheme["effects"]>) => onChange({ ...theme, effects: { ...theme.effects, ...patch } });
  const patchDecoration = (index: number, patch: Partial<Decoration>) => onChange({ ...theme, decorations: theme.decorations.map((item, itemIndex) => itemIndex === index ? { ...item, ...patch } : item) });

  function startDrag(event: DragEvent, item: DragItem) {
    if (disabled) { event.preventDefault(); return; }
    event.stopPropagation();
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("application/x-aurora-layout", JSON.stringify(item));
    setDragging(item);
  }

  function dropItem(event: DragEvent): DragItem | null {
    event.preventDefault();
    event.stopPropagation();
    if (disabled) return null;
    // Only drags started in this trusted editor are used. External HTML/text is ignored.
    const result = dragging;
    setDragging(null);
    return result;
  }

  function moveNav(source: NavId, target: NavId) {
    patchLayout({ navOrder: moveItem(theme.layout.navOrder, source, target) });
    setDragNotice(`“${NAV_NAMES[source]}” movido na navegação.`);
  }

  function moveSection(source: SectionId, target: SectionId) {
    patchLayout({ sectionOrder: moveItem(theme.layout.sectionOrder, source, target) });
    setDragNotice(`“${SECTION_NAMES[source]}” movido na página.`);
  }

  function sideDrop(event: DragEvent, side: "left" | "right") {
    const item = dropItem(event);
    if (item?.kind === "sidebar") { patchLayout({ sidebar: side }); setDragNotice(`Menu movido para a ${side === "left" ? "esquerda" : "direita"}.`); }
    if (item?.kind === "status") { patchLayout({ statusSide: side }); setDragNotice(`Painel de status movido para a ${side === "left" ? "esquerda" : "direita"}.`); }
  }

  function pointerStart(event: PointerEvent<HTMLButtonElement>, index: number) {
    if (disabled || event.button !== 0 || !area.current) return;
    event.preventDefault();
    event.stopPropagation();
    setSelected(index);
    setTab("elements");
    const bounds = area.current.getBoundingClientRect();
    const item = theme.decorations[index];
    pointer.current = { pointerId: event.pointerId, index, startX: event.clientX, startY: event.clientY, x: item.x, y: item.y, freeWidth: bounds.width * (1 - item.width / 100), freeHeight: Math.max(0, bounds.height - item.height * .55) };
    event.currentTarget.setPointerCapture(event.pointerId);
    setDragPosition({ index, x: item.x, y: item.y });
  }

  function pointerMove(event: PointerEvent<HTMLButtonElement>) {
    const current = pointer.current;
    if (!current || event.pointerId !== current.pointerId) return;
    setDragPosition({ index: current.index, x: Math.round(clamp(current.x + (current.freeWidth ? (event.clientX - current.startX) / current.freeWidth * 100 : 0), 0, 100)), y: Math.round(clamp(current.y + (current.freeHeight ? (event.clientY - current.startY) / current.freeHeight * 100 : 0), 0, 100)) });
  }

  function pointerEnd(event: PointerEvent<HTMLButtonElement>, cancel = false) {
    const current = pointer.current;
    if (!current || current.pointerId !== event.pointerId) return;
    pointer.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    if (!cancel && !disabled) {
      const x = Math.round(clamp(current.x + (current.freeWidth ? (event.clientX - current.startX) / current.freeWidth * 100 : 0), 0, 100));
      const y = Math.round(clamp(current.y + (current.freeHeight ? (event.clientY - current.startY) / current.freeHeight * 100 : 0), 0, 100));
      patchDecoration(current.index, { x, y });
      setDragNotice(`Elemento na posição horizontal ${x}% e vertical ${y}%.`);
    }
    setDragPosition(null);
  }

  function addDecoration(type: "text" | "image") {
    if (theme.decorations.length >= 6 || (type === "image" && !pngIds.length)) return;
    const next: Decoration = { type, text: type === "text" ? "Seu texto aqui" : "", asset: type === "image" ? pngIds[0] : null, align: "center", x: 50, y: 50, width: 35, height: 64, fontSize: 20 };
    onChange({ ...theme, decorations: [...theme.decorations, next] });
    setSelected(theme.decorations.length);
    setTab("elements");
  }

  function removeDecoration(index: number) {
    const removed = theme.decorations[index];
    const decorations = theme.decorations.filter((_, itemIndex) => itemIndex !== index);
    const assets = { ...theme.assets };
    if (removed.asset && theme.media.background !== removed.asset && theme.media.logo !== removed.asset && !Object.values(theme.icons).includes(`asset:${removed.asset}`) && !decorations.some((item) => item.asset === removed.asset)) delete assets[removed.asset];
    onChange({ ...theme, assets, decorations });
    setSelected(0);
  }

  const previewStyle = {
    "--tv-background": theme.colors.background, "--tv-panel": theme.colors.panel, "--tv-text": theme.colors.text,
    "--tv-accent": theme.colors.accent, "--tv-accent-text": theme.colors.accentText, "--tv-border": theme.colors.border,
    "--tv-radius": `${theme.layout.radius * .5}px`, "--tv-scale": theme.typography.scale, fontFamily: FONT_FAMILIES[theme.typography.font],
    border: `${theme.window.borderWidth}px solid ${theme.window.frameColor}`,
    borderRadius: `${theme.window.outerRadius * .65}px`,
  } as CSSProperties;
  const bodyStyle = { gridTemplateColumns: theme.layout.sidebar === "left" ? `${theme.layout.sidebarWidth * .42}px minmax(0, 1fr)` : `minmax(0, 1fr) ${theme.layout.sidebarWidth * .42}px`, padding: `${theme.window.contentInset * .4}px` };

  return <div className="theme-visual-editor">
    <div className="tv-identity"><label>Nome do tema<input type="text" value={theme.name} maxLength={60} disabled={disabled} onChange={(event) => onChange({ ...theme, name: event.target.value })} /></label><label>Identificador<input type="text" value={theme.id} maxLength={48} disabled={disabled} spellCheck={false} onChange={(event) => onChange({ ...theme, id: event.target.value.toLowerCase().replace(/[^a-z0-9-]/g, "") })} /><small>Sem espaços. Usado ao salvar e compartilhar.</small></label></div>
    <div className="tv-preview-heading"><div><strong>Prévia local</strong><p>Arraste blocos entre áreas; textos e imagens se movem livremente no painel decorativo.</p></div><span>O launcher só muda em “Testar por 10 s”</span></div>
    <div className="tv-preview" data-dragging={dragging?.kind ?? "none"}>
      <div className="tv-side-drop tv-side-drop-left" onDragOver={(event) => { if (dragging?.kind === "sidebar" || dragging?.kind === "status") event.preventDefault(); }} onDrop={(event) => sideDrop(event, "left")}>Solte à esquerda</div>
      <div className={`tv-window tv-effect-${theme.effects.background}`} style={previewStyle}>
        {theme.media.background && urls[theme.media.background] && <img className="tv-window-background" src={urls[theme.media.background]} alt="" />}
        {backgroundAsset && backgroundAsset.mime !== "image/png" && <span className="tv-local-media-badge">{backgroundAsset.mime === "image/gif" ? "GIF local" : "Vídeo local"} · animação no teste de 10 s</span>}
        <div className="tv-titlebar" style={{ height: theme.window.titlebarHeight * .55, backgroundColor: theme.window.titlebarColor, color: theme.window.titlebarText }}><span>✦ Aurora · Prévia</span><span aria-hidden="true">—　□　×</span></div>
        <div className="tv-body" style={bodyStyle}>
          <aside className="tv-sidebar" style={{ gridColumn: theme.layout.sidebar === "left" ? 1 : 2 }} draggable={!disabled} onDragStart={(event) => startDrag(event, { kind: "sidebar" })} onDragEnd={() => setDragging(null)}>
            <span className="tv-drag-label">⠿ Menu</span><div className="tv-brand">{theme.media.logo && urls[theme.media.logo] ? <img src={urls[theme.media.logo]} alt="" /> : <b>✦</b>}<strong>Aurora</strong></div>
            {theme.layout.navOrder.map((nav, index) => <button className={index === 0 ? "tv-nav is-current" : "tv-nav"} key={nav} type="button" disabled={disabled} draggable={!disabled} onDragStart={(event) => startDrag(event, { kind: "nav", id: nav })} onDragEnd={() => setDragging(null)} onDragOver={(event) => { if (dragging?.kind === "nav") event.preventDefault(); }} onDrop={(event) => { const item = dropItem(event); if (item?.kind === "nav") moveNav(item.id, nav); }} title={`Arrastar ${NAV_NAMES[nav]} para reordenar`}><span>{theme.icons[nav].startsWith("asset:") && urls[theme.icons[nav].slice(6)] ? <img src={urls[theme.icons[nav].slice(6)]} alt="" /> : ICONS[theme.icons[nav]] ?? "◈"}</span>{NAV_NAMES[nav]}<i aria-hidden="true">⠿</i></button>)}
          </aside>
          <div className="tv-page" style={{ gridColumn: theme.layout.sidebar === "left" ? 2 : 1 }}>
            {theme.layout.sectionOrder.map((section) => <section className={`tv-section tv-section-${section}`} key={section} onDragOver={(event) => { if (dragging?.kind === "section") event.preventDefault(); }} onDrop={(event) => { const item = dropItem(event); if (item?.kind === "section") moveSection(item.id, section); }}>
              <button className="tv-section-handle" type="button" disabled={disabled} draggable={!disabled} onDragStart={(event) => startDrag(event, { kind: "section", id: section })} onDragEnd={() => setDragging(null)} title={`Arrastar ${SECTION_NAMES[section]}`}><span>⠿</span>{SECTION_NAMES[section]}</button>
              {section === "header" && <div className="tv-sample-heading"><small>SEU PRÓXIMO MUNDO</small><strong>Meus modpacks</strong><span>Aventura, do seu jeito.</span></div>}
              {section === "content" && <div className="tv-content" data-status-side={theme.layout.statusSide}><div className="tv-sample-cards"><div><i aria-hidden="true">◈</i><strong>Meu modpack</strong><span>Pronto para jogar</span></div><div><i aria-hidden="true">◇</i><strong>Outro mundo</strong><span>Mais uma aventura</span></div></div><aside className="tv-status" draggable={!disabled} onDragStart={(event) => startDrag(event, { kind: "status" })} onDragEnd={() => setDragging(null)}><span>⠿ Status</span><strong>Pronto</strong><small>Java · Memória · Jogo</small></aside></div>}
              {section === "decorations" && <div ref={area} className="tv-decoration-area" style={{ height: theme.layout.decorationHeight * .55 }}>
                {!theme.decorations.length && <span className="tv-decoration-placeholder">Adicione um texto ou imagem em “Elementos”</span>}
                {theme.decorations.map((item, index) => {
                  const position = dragPosition?.index === index ? dragPosition : item;
                  const height = Math.min(item.height, theme.layout.decorationHeight) * .55;
                  return <button key={index} type="button" disabled={disabled} className={`tv-decoration${selectedIndex === index ? " is-selected" : ""}`} style={{ left: `${position.x * (100 - item.width) / 100}%`, top: `${position.y * Math.max(0, theme.layout.decorationHeight - item.height) * .55 / 100}px`, width: `${item.width}%`, height, fontSize: item.fontSize * .55, textAlign: item.align, justifyContent: item.align === "left" ? "flex-start" : item.align === "right" ? "flex-end" : "center" }} aria-label={`Mover elemento ${index + 1}: ${item.text || "imagem"}`} onPointerDown={(event) => pointerStart(event, index)} onPointerMove={pointerMove} onPointerUp={(event) => pointerEnd(event)} onPointerCancel={(event) => pointerEnd(event, true)} onLostPointerCapture={(event) => pointerEnd(event, true)} onClick={() => { setSelected(index); setTab("elements"); }} onKeyDown={(event) => { const arrows: Record<string, [number, number]> = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }; const direction = arrows[event.key]; if (!direction) return; event.preventDefault(); const step = event.shiftKey ? 10 : 2; patchDecoration(index, { x: clamp(item.x + direction[0] * step, 0, 100), y: clamp(item.y + direction[1] * step, 0, 100) }); }}>
                    {item.type === "image" && item.asset && urls[item.asset] && <img src={urls[item.asset]} alt={item.text} />}{item.type === "text" && <span>{item.text}</span>}
                  </button>;
                })}
              </div>}
            </section>)}
          </div>
        </div>
      </div>
      <div className="tv-side-drop tv-side-drop-right" onDragOver={(event) => { if (dragging?.kind === "sidebar" || dragging?.kind === "status") event.preventDefault(); }} onDrop={(event) => sideDrop(event, "right")}>Solte à direita</div>
    </div>
    <p className="tv-preview-hint">Use também os controles abaixo ou as setas do teclado em um elemento selecionado. Os botões desta miniatura apenas organizam o tema.</p>
    <div className="tv-tabs" role="tablist" aria-label="Controles do editor visual" onKeyDown={(event) => { if (disabled || !["ArrowRight", "ArrowLeft", "Home", "End"].includes(event.key)) return; event.preventDefault(); const tabs = ["layout", "colors", "window", "motion", "cursor", "elements"] as const; const index = tabs.indexOf(tab); const next = event.key === "Home" ? 0 : event.key === "End" ? 5 : (index + (event.key === "ArrowRight" ? 1 : -1) + 6) % 6; setTab(tabs[next]); event.currentTarget.querySelectorAll<HTMLButtonElement>("button")[next]?.focus(); }}>{([["layout", "Organização"], ["colors", "Cores e letras"], ["window", "Janela e moldura"], ["motion", "Efeitos"], ["cursor", "Cursor"], ["elements", "Elementos"]] as const).map(([value, label]) => <button role="tab" id={`${editorId}-${value}-tab`} aria-controls={`${editorId}-controls`} tabIndex={tab === value ? 0 : -1} aria-selected={tab === value} type="button" className={tab === value ? "is-active" : ""} key={value} disabled={disabled} onClick={() => setTab(value)}>{label}</button>)}</div>
    <div className="tv-controls" role="tabpanel" id={`${editorId}-controls`} aria-labelledby={`${editorId}-${tab}-tab`}>
      {tab === "layout" && <>
        <div className="tv-field-grid"><label>Posição do menu<select value={theme.layout.sidebar} disabled={disabled} onChange={(event) => patchLayout({ sidebar: event.target.value as "left" | "right" })}><option value="left">Esquerda</option><option value="right">Direita</option></select></label><label>Estado do Core<select value={theme.layout.statusSide} disabled={disabled} onChange={(event) => patchLayout({ statusSide: event.target.value as "left" | "right" })}><option value="left">Esquerda</option><option value="right">Direita</option></select></label><label>Espaçamento<select value={theme.layout.density} disabled={disabled} onChange={(event) => patchLayout({ density: event.target.value as "comfortable" | "compact" })}><option value="comfortable">Confortável</option><option value="compact">Compacto</option></select></label></div>
        <div className="tv-field-grid"><Slider label="Largura do menu" value={theme.layout.sidebarWidth} min={180} max={300} unit=" px" disabled={disabled} onChange={(sidebarWidth) => patchLayout({ sidebarWidth })} /><Slider label="Cantos dos painéis" value={theme.layout.radius} min={0} max={24} unit=" px" disabled={disabled} onChange={(radius) => patchLayout({ radius })} /><Slider label="Altura do painel decorativo" value={theme.layout.decorationHeight} min={80} max={400} unit=" px" disabled={disabled} onChange={(decorationHeight) => patchLayout({ decorationHeight })} /></div>
        <p className="tv-help">O painel de estado do Core aparece nas áreas de Instâncias e Java quando há informações de conexão disponíveis.</p>
        <div className="tv-order-groups"><div><h4>Ordem da navegação</h4>{theme.layout.navOrder.map((nav, index) => <div className="tv-order-row" key={nav}><span>{NAV_NAMES[nav]}</span><button type="button" className="secondary compact" disabled={disabled || index === 0} aria-label={`Subir ${NAV_NAMES[nav]}`} onClick={() => moveNav(nav, theme.layout.navOrder[index - 1])}>↑</button><button type="button" className="secondary compact" disabled={disabled || index === 3} aria-label={`Descer ${NAV_NAMES[nav]}`} onClick={() => moveNav(nav, theme.layout.navOrder[index + 1])}>↓</button></div>)}</div><div><h4>Ordem dos blocos</h4>{theme.layout.sectionOrder.map((section, index) => <div className="tv-order-row" key={section}><span>{SECTION_NAMES[section]}</span><button type="button" className="secondary compact" disabled={disabled || index === 0} aria-label={`Subir ${SECTION_NAMES[section]}`} onClick={() => moveSection(section, theme.layout.sectionOrder[index - 1])}>↑</button><button type="button" className="secondary compact" disabled={disabled || index === 2} aria-label={`Descer ${SECTION_NAMES[section]}`} onClick={() => moveSection(section, theme.layout.sectionOrder[index + 1])}>↓</button></div>)}</div></div>
      </>}
      {tab === "colors" && <>
        <div className="tv-field-grid tv-colors">{([["background", "Fundo"], ["panel", "Painéis"], ["text", "Texto"], ["muted", "Texto secundário"], ["accent", "Cor de destaque"], ["accentText", "Texto dos botões"], ["border", "Bordas"]] as const).map(([key, label]) => <ColorField key={key} label={label} value={theme.colors[key]} disabled={disabled} onChange={(value) => onChange({ ...theme, colors: { ...theme.colors, [key]: value } })} />)}</div>
        <p className="tv-help">Cores precisam manter a leitura. O contraste é verificado ao salvar ou testar o tema.</p>
        <div className="tv-field-grid"><label>Fonte<select value={theme.typography.font} disabled={disabled} onChange={(event) => onChange({ ...theme, typography: { ...theme.typography, font: event.target.value as AuroraTheme["typography"]["font"] } })}><option value="system">Sistema</option><option value="rounded">Arredondada</option><option value="serif">Clássica</option><option value="mono">Monoespaçada</option></select></label><Slider label="Tamanho das letras" value={theme.typography.scale} min={.9} max={1.15} step={.01} unit="×" disabled={disabled} onChange={(scale) => onChange({ ...theme, typography: { ...theme.typography, scale } })} /></div>
        <h4>Ícones do menu</h4><div className="tv-field-grid">{theme.layout.navOrder.map((nav) => <label key={nav}>{NAV_NAMES[nav]}<select value={theme.icons[nav]} disabled={disabled} onChange={(event) => onChange({ ...theme, icons: { ...theme.icons, [nav]: event.target.value } })}>{THEME_ICON_NAMES.map((icon) => <option key={icon} value={icon}>{ICON_NAMES[icon]}</option>)}{pngIds.map((asset) => <option key={asset} value={`asset:${asset}`}>Imagem: {asset}</option>)}</select></label>)}</div>
      </>}
      {tab === "window" && <><div className="tv-field-grid tv-colors"><ColorField label="Fundo da barra superior" value={theme.window.titlebarColor} disabled={disabled} onChange={(titlebarColor) => patchWindow({ titlebarColor })} /><ColorField label="Texto da barra superior" value={theme.window.titlebarText} disabled={disabled} onChange={(titlebarText) => patchWindow({ titlebarText })} /><ColorField label="Cor da moldura" value={theme.window.frameColor} disabled={disabled} onChange={(frameColor) => patchWindow({ frameColor })} /></div><div className="tv-field-grid"><Slider label="Altura da barra superior" value={theme.window.titlebarHeight} min={36} max={64} unit=" px" disabled={disabled} onChange={(titlebarHeight) => patchWindow({ titlebarHeight })} /><Slider label="Espessura da moldura" value={theme.window.borderWidth} min={0} max={3} unit=" px" disabled={disabled} onChange={(borderWidth) => patchWindow({ borderWidth })} /><Slider label="Arredondamento da janela" value={theme.window.outerRadius} min={0} max={24} unit=" px" disabled={disabled} onChange={(outerRadius) => patchWindow({ outerRadius })} /><Slider label="Margem interna" value={theme.window.contentInset} min={0} max={24} unit=" px" disabled={disabled} onChange={(contentInset) => patchWindow({ contentInset })} /></div><p className="tv-help">Minimizar, maximizar, fechar e recuperar a aparência continuam sendo controles oficiais do Aurora.</p></>}
      {tab === "cursor" && <>
        <div className="tv-field-grid"><label>Estilo do mouse<select value={theme.cursor.style} disabled={disabled} onChange={(event) => onChange({ ...theme, cursor: { ...theme.cursor, style: event.target.value as AuroraTheme["cursor"]["style"] } })}><option value="system">Mouse do sistema</option><option value="pixel">Ponteiro em pixels</option><option value="ring">Anel</option><option value="crosshair">Mira</option></select></label><ColorField label="Cor do mouse" value={theme.cursor.color} disabled={disabled} onChange={(color) => onChange({ ...theme, cursor: { ...theme.cursor, color } })} /><Slider label="Tamanho do mouse" value={theme.cursor.size} min={16} max={40} unit=" px" disabled={disabled} onChange={(size) => onChange({ ...theme, cursor: { ...theme.cursor, size } })} /><label>Rastro leve<select value={theme.cursor.trail} disabled={disabled} onChange={(event) => onChange({ ...theme, cursor: { ...theme.cursor, trail: event.target.value as AuroraTheme["cursor"]["trail"] } })}><option value="none">Sem rastro</option><option value="sparkle">Pequenos brilhos</option></select></label></div>
        <div className="tv-cursor-sample"><div><span style={{ color: theme.cursor.color, fontSize: theme.cursor.size }}>{theme.cursor.style === "ring" ? "◯" : theme.cursor.style === "crosshair" ? "⌖" : "↖"}</span>{theme.cursor.trail === "sparkle" && <i style={{ color: theme.cursor.color }}>✦ · ˙</i>}</div><p>Exemplo do estilo escolhido. O cursor completo aparece no teste do tema.</p></div>
        <p className="tv-help">A personalização do mouse vale somente dentro do Aurora. Confirmação, recuperação e controles oficiais da janela mantêm o cursor do sistema. Brilhos respeitam movimento reduzido.</p>
      </>}
      {tab === "motion" && <><div className="tv-field-grid"><label>Cenário<select value={theme.effects.background} disabled={disabled} onChange={(event) => patchEffect({ background: event.target.value as AuroraTheme["effects"]["background"] })}><option value="none">Sem efeito</option><option value="aurora">Aurora boreal</option><option value="water">Água</option><option value="sakura">Cerejeira e pétalas</option></select></label><label>Transição ao trocar tema<select value={theme.effects.transition} disabled={disabled} onChange={(event) => patchEffect({ transition: event.target.value as AuroraTheme["effects"]["transition"] })}><option value="none">Sem transição</option><option value="fade">Aparecer suavemente</option><option value="slide">Deslizar</option><option value="zoom">Aproximar suavemente</option></select></label></div><div className="tv-field-grid"><Slider label="Intensidade" value={theme.effects.intensity} min={0} max={1} step={.05} disabled={disabled} onChange={(intensity) => patchEffect({ intensity })} /><Slider label="Velocidade" value={theme.effects.speed} min={.25} max={2} step={.05} unit="×" disabled={disabled} onChange={(speed) => patchEffect({ speed })} /><Slider label="Efeito de vidro" value={theme.effects.glass} min={0} max={16} unit=" px" disabled={disabled} onChange={(glass) => patchEffect({ glass })} /><Slider label="Duração da transição" value={theme.effects.transitionMs} min={100} max={700} step={25} unit=" ms" disabled={disabled} onChange={(transitionMs) => patchEffect({ transitionMs })} /></div><p className="tv-help">A miniatura mostra a composição. A animação completa aparece no teste de 10 segundos e respeita a opção de movimento reduzido do sistema.</p></>}
      {tab === "elements" && <><div className="tv-elements-heading"><div><h4>Textos e imagens</h4><p>Até seis elementos no painel decorativo.</p></div><button type="button" className="secondary compact" disabled={disabled || theme.decorations.length >= 6} onClick={() => addDecoration("text")}>+ Texto</button><button type="button" className="secondary compact" disabled={disabled || theme.decorations.length >= 6 || Object.keys(theme.assets).length >= 4} onClick={onImportImage}>+ Imagem local</button>{pngIds.length > 0 && <button type="button" className="secondary compact" disabled={disabled || theme.decorations.length >= 6} onClick={() => addDecoration("image")}>+ Imagem já anexada</button>}</div>{!pngIds.length && <p className="tv-help">Use “+ Imagem local” para escolher um arquivo do computador. Ela será incluída no código do tema.</p>}{theme.decorations.length > 0 && <div className="tv-element-picker" role="group" aria-label="Selecionar elemento">{theme.decorations.map((item, index) => <button className={selectedIndex === index ? "is-selected" : ""} key={index} type="button" disabled={disabled} onClick={() => setSelected(index)}> {index + 1}. {item.type === "text" ? "Texto" : "Imagem"}</button>)}</div>}{decoration && <><div className="tv-field-grid"><label>{decoration.type === "image" ? "Descrição da imagem" : "Texto"}<input type="text" value={decoration.text} maxLength={160} disabled={disabled} onChange={(event) => patchDecoration(selectedIndex, { text: event.target.value })} /></label>{decoration.type === "image" && <label>Imagem<select value={decoration.asset ?? ""} disabled={disabled} onChange={(event) => patchDecoration(selectedIndex, { asset: event.target.value })}>{pngIds.map((asset) => <option value={asset} key={asset}>{asset}</option>)}</select></label>}<label>Alinhamento do texto<select value={decoration.align} disabled={disabled} onChange={(event) => patchDecoration(selectedIndex, { align: event.target.value as Decoration["align"] })}><option value="left">Esquerda</option><option value="center">Centro</option><option value="right">Direita</option></select></label></div><div className="tv-field-grid"><Slider label="Posição horizontal" value={decoration.x} min={0} max={100} unit="%" disabled={disabled} onChange={(x) => patchDecoration(selectedIndex, { x })} /><Slider label="Posição vertical" value={decoration.y} min={0} max={100} unit="%" disabled={disabled} onChange={(y) => patchDecoration(selectedIndex, { y })} /><Slider label="Largura" value={decoration.width} min={8} max={100} unit="%" disabled={disabled} onChange={(width) => patchDecoration(selectedIndex, { width })} /><Slider label="Altura" value={decoration.height} min={24} max={160} unit=" px" disabled={disabled} onChange={(height) => patchDecoration(selectedIndex, { height })} /><Slider label="Tamanho do texto" value={decoration.fontSize} min={12} max={56} unit=" px" disabled={disabled} onChange={(fontSize) => patchDecoration(selectedIndex, { fontSize })} /></div><button className="danger compact tv-remove-element" type="button" disabled={disabled} onClick={() => removeDecoration(selectedIndex)}>Remover elemento {selectedIndex + 1}</button></>}{!decoration && <p className="tv-help">Adicione um elemento para ajustar tamanho e posição.</p>}</>}
    </div>
    <p className="tv-announcement" role="status">{dragNotice}</p>
  </div>;
}
