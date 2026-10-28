import { useEffect, useId, useMemo, useRef, useState, type CSSProperties } from "react";
import type { AuroraTheme } from "./model";
import "./scene.css";

// Fixed artwork coordinates; theme documents can choose colors, never SVG markup.
const SAKURA_BLOSSOMS = [
  [720, 308], [757, 328], [788, 302], [824, 341], [862, 358], [900, 337], [945, 375], [978, 350], [1015, 397], [1056, 372],
  [844, 216], [880, 239], [915, 214], [950, 247], [984, 218], [1021, 261], [1063, 244], [1095, 281], [1127, 248], [1153, 213],
  [1195, 174], [1225, 207], [1250, 166], [1289, 198], [1317, 157], [1354, 186], [1387, 147], [1413, 172], [1440, 133], [1480, 165],
  [1283, 294], [1325, 268], [1358, 307], [1399, 283], [1435, 328], [1470, 306], [1493, 349], [1226, 347], [1178, 324], [1104, 343],
] as const;

function useMotionPaused(): boolean {
  const [paused, setPaused] = useState(() => document.hidden || window.matchMedia("(prefers-reduced-motion: reduce)").matches);
  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setPaused(document.hidden || media.matches);
    document.addEventListener("visibilitychange", update);
    media.addEventListener("change", update);
    update();
    return () => { document.removeEventListener("visibilitychange", update); media.removeEventListener("change", update); };
  }, []);
  return paused;
}

/** Object URLs come exclusively from already-validated embedded bytes. */
function useEmbeddedResources(theme: AuroraTheme, ids: string[], onError?: (message: string) => void): Record<string, string> {
  const selection = [...new Set(ids)].sort().join("|");
  const [resources, setResources] = useState<{ owner: AuroraTheme; urls: Record<string, string> } | null>(null);
  useEffect(() => {
    const next: Record<string, string> = {};
    try {
      for (const id of selection ? selection.split("|") : []) {
        const asset = theme.assets[id];
        const bytes = Uint8Array.from(atob(asset.data), (char) => char.charCodeAt(0));
        next[id] = URL.createObjectURL(new Blob([bytes], { type: asset.mime }));
      }
      setResources({ owner: theme, urls: next });
    } catch {
      for (const url of Object.values(next)) URL.revokeObjectURL(url);
      onError?.("Não foi possível carregar os recursos visuais deste tema.");
    }
    return () => { for (const url of Object.values(next)) URL.revokeObjectURL(url); };
  }, [theme, selection]);
  return resources?.owner === theme ? resources.urls : {};
}

function Landscape({ kind, id }: { kind: AuroraTheme["effects"]["background"]; id: string }) {
  if (kind === "none") return null;
  if (kind === "aurora") return <div className="theme-aurora"><i /><i /><i /><div className="theme-stars" /></div>;
  if (kind === "water") return <svg className="theme-landscape theme-lake" viewBox="0 0 1440 900" preserveAspectRatio="xMidYMid slice" focusable="false">
    <defs><linearGradient id={`${id}-water`} x1="0" y1="0" x2="0" y2="1"><stop stopColor="var(--theme-accent)" stopOpacity=".06" /><stop offset="1" stopColor="var(--theme-accent)" stopOpacity=".28" /></linearGradient><radialGradient id={`${id}-moon`}><stop stopColor="var(--theme-text)" stopOpacity=".3" /><stop offset="1" stopColor="var(--theme-accent)" stopOpacity="0" /></radialGradient></defs>
    <circle cx="1130" cy="220" r="165" fill={`url(#${id}-moon)`} /><circle cx="1130" cy="220" r="44" fill="var(--theme-text)" opacity=".32" />
    <path d="M0 500 180 320 335 455 535 270 760 465 930 350 1160 480 1330 315 1440 430V680H0Z" fill="var(--theme-panel)" opacity=".8" />
    <path d="M0 570Q310 495 650 555T1440 545V900H0Z" fill={`url(#${id}-water)`} />
    <g className="theme-water-lines" fill="none" stroke="var(--theme-accent)" strokeWidth="1.5"><path d="M760 620q155-18 340 0M200 687q220-19 440 0M890 748q150-16 360 0M100 823q190-15 390 0M695 864q220-18 560 0" /><path d="M990 584h250m-310 30h340m-255 39h190" opacity=".6" /></g>
    <ellipse className="theme-water-ripple" cx="1080" cy="740" rx="190" ry="20" fill="none" stroke="var(--theme-accent)" opacity=".18" />
  </svg>;
  return <><svg className="theme-landscape theme-sakura-tree" viewBox="0 0 1440 900" preserveAspectRatio="xMidYMid slice" focusable="false">
    <defs><radialGradient id={`${id}-sakura`}><stop stopColor="var(--theme-accent)" stopOpacity=".12" /><stop offset="1" stopColor="var(--theme-accent)" stopOpacity="0" /></radialGradient>
      <g id={`${id}-blossom`}><g fill="var(--theme-accent)">{[0, 72, 144, 216, 288].map((angle) => <path key={angle} d="M0-2C-7-6-9-14-3-17L0-14 3-17C9-14 7-6 0-2Z" transform={`rotate(${angle})`} />)}</g><circle r="2.4" fill="var(--theme-text)" /></g>
    </defs>
    <ellipse cx="1200" cy="310" rx="440" ry="280" fill={`url(#${id}-sakura)`} />
    <g fill="none" stroke="var(--theme-muted)" strokeLinecap="round" strokeLinejoin="round">
      <path d="M1490 940C1350 790 1285 660 1280 475C1275 380 1225 292 1153 213" strokeWidth="21" opacity=".34" />
      <path d="M1285 590C1170 450 1055 400 862 358M1250 390C1155 290 1020 240 880 239M1280 475C1300 350 1360 240 1440 133M1320 365Q1420 355 1493 349" strokeWidth="10" opacity=".4" />
      <path d="M1050 409Q965 330 900 337M947 377Q830 292 720 308M1000 391Q935 388 945 375M1120 441Q1075 378 1056 372M1110 288Q1030 205 984 218M1025 260Q950 198 915 214M963 251Q895 210 844 216M1200 335Q1207 220 1195 174M1287 399Q1260 275 1225 207M1307 318Q1288 237 1250 166M1368 230Q1330 162 1317 157M1405 178Q1450 150 1480 165M1350 291Q1380 270 1399 283M1372 356Q1350 310 1358 307M1431 350Q1440 305 1470 306M1240 370Q1200 326 1178 324" strokeWidth="3.2" opacity=".48" />
    </g>
    {SAKURA_BLOSSOMS.map(([x, y], index) => <use key={index} href={`#${id}-blossom`} transform={`translate(${x} ${y}) rotate(${index * 37}) scale(${0.7 + (index % 5) * 0.12})`} opacity={0.5 + (index % 4) * 0.08} />)}
  </svg><div className="theme-petals">{Array.from({ length: 12 }, (_, i) => <i key={i} style={{ "--petal-index": i, left: `${6 + (i * 29) % 91}%`, animationDelay: `${-i * 2.7}s` } as CSSProperties} />)}</div></>;
}

function SceneContent({ theme, onError }: { theme: AuroraTheme; onError?: (message: string) => void }) {
  const id = useId().replaceAll(":", "");
  const paused = useMotionPaused();
  const resources = useEmbeddedResources(theme, theme.media.background ? [theme.media.background] : [], onError);
  const video = useRef<HTMLVideoElement>(null);
  const background = theme.media.background ? theme.assets[theme.media.background] : null;
  const source = theme.media.background ? resources[theme.media.background] : null;
  const fail = () => onError?.("Não foi possível abrir um recurso visual deste tema. O Aurora pode restaurar a aparência original.");
  useEffect(() => {
    const element = video.current;
    if (!element) return;
    if (paused) element.pause();
    else void element.play().catch(() => { /* A codec/autoplay restriction leaves a still background. */ });
  }, [paused, source]);
  return <div className={`theme-scene theme-scene-${theme.effects.background}`} data-paused={paused ? "true" : "false"} aria-hidden="true">
    {source && background && (background.mime === "image/png"
      ? <img className="theme-background-media" src={source} alt="" onError={fail} />
      : <video ref={video} className="theme-background-media" src={source} muted loop playsInline preload="auto" onError={fail} />)}
    <div className="theme-scene-effect"><Landscape kind={theme.effects.background} id={id} /></div>
    <div className="theme-scene-vignette" />
  </div>;
}

/** Decorative content stays in normal layout, with no buttons, links or overlays. */
export function ThemeDecorations({ theme, onError }: { theme: AuroraTheme; onError?: (message: string) => void }) {
  const resources = useEmbeddedResources(theme, theme.decorations.flatMap((entry) => entry.type === "image" && entry.asset ? [entry.asset] : []), onError);
  if (theme.decorations.length === 0) return null;
  return <div className="theme-decorations" aria-label="Elementos visuais do tema">{theme.decorations.map((decoration, i) => <div key={i} className={`theme-decoration theme-decoration-${decoration.align}`}>
    {decoration.type === "image" && decoration.asset && resources[decoration.asset] ? <img src={resources[decoration.asset]} alt={decoration.text} onError={() => onError?.("Não foi possível abrir uma imagem deste tema.")} /> : null}
    {decoration.type === "text" && decoration.text && <span>{decoration.text}</span>}
  </div>)}</div>;
}

export function ThemeScene({ theme, onError }: { theme: AuroraTheme; onError?: (message: string) => void }) {
  // A distinct trusted React subtree replays the short transition when a theme changes.
  const instance = useMemo(() => ({}), [theme]);
  const serial = useRef(0);
  const previous = useRef(instance);
  if (previous.current !== instance) { serial.current += 1; previous.current = instance; }
  return <SceneContent key={serial.current} theme={theme} onError={onError} />;
}
