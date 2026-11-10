import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { DEFAULT_THEME, parseTheme, type AuroraTheme, type ThemeLibrary, type ThemePreview } from "./model";
import { themeCommand } from "./transport";
import { validateThemeGif } from "./gif";
import "./shell.css";

const EMPTY: ThemeLibrary = { themes: [], selectedThemeId: null, bindings: {}, automatic: false, recovered: false, persistent: false };
type Pending = ThemePreview & { theme: AuroraTheme; instanceId: string | null };
type ThemesContext = {
  library: ThemeLibrary; theme: AuroraTheme; busy: boolean; error: string;
  setInstance: (id: string) => void;
  save: (json: string) => Promise<void>; preview: (id: string, instanceId: string | null) => Promise<void>;
  remove: (id: string) => Promise<void>; reset: () => Promise<void>;
  automatic: (enabled: boolean) => Promise<void>; unbind: (instanceId: string) => Promise<void>;
  mediaFailed: (message: string) => void;
};
const Context = createContext<ThemesContext | null>(null);

function publicError(error: unknown) {
  const text = error instanceof Error ? error.message : typeof error === "string" ? error : "Não foi possível alterar a aparência. Tente novamente.";
  return text.length <= 240 && !/[\r\n]/.test(text) ? text : "Não foi possível alterar a aparência. Verifique o tema e tente novamente.";
}

async function preloadTheme(theme: AuroraTheme) {
  // Check all bundled media before starting the ten-second native deadline.
  for (const asset of Object.values(theme.assets)) {
    if (asset.mime === "image/gif") { await validateThemeGif(asset); continue; }
    const binary = atob(asset.data);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index++) bytes[index] = binary.charCodeAt(index);
    const url = URL.createObjectURL(new Blob([bytes], { type: asset.mime }));
    try {
      await new Promise<void>((resolve, reject) => {
        const element = asset.mime === "image/png" ? new Image() : document.createElement("video");
        const timer = setTimeout(() => finish(new Error("A mídia do tema demorou para carregar. Use um arquivo menor ou outro formato.")), 8_000);
        function finish(error?: Error) {
          clearTimeout(timer);
          element.onload = null;
          element.onerror = null;
          if (element instanceof HTMLVideoElement) {
            element.onloadeddata = null;
            element.onloadedmetadata = null;
            element.pause();
            element.removeAttribute("src");
            element.load();
          }
          if (error) reject(error); else resolve();
        }
        element.onerror = () => finish(new Error("Não foi possível reproduzir a mídia do tema. Escolha outro arquivo."));
        if (element instanceof HTMLVideoElement) {
          element.muted = true;
          element.preload = "auto";
          element.onloadedmetadata = () => {
            if (element.videoWidth < 1 || element.videoHeight < 1 || element.videoWidth > 1920 || element.videoHeight > 1080
              || element.videoWidth * element.videoHeight > 2_073_600 || !Number.isFinite(element.duration) || element.duration <= 0 || element.duration > 180) {
              finish(new Error("Use um vídeo de até 1920×1080 e três minutos, com duração definida."));
            }
          };
          element.onloadeddata = () => finish();
        } else element.onload = () => finish();
        element.src = url;
      });
    } finally { URL.revokeObjectURL(url); }
  }
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [library, setLibrary] = useState<ThemeLibrary>(EMPTY);
  const [theme, setTheme] = useState<AuroraTheme>(DEFAULT_THEME);
  const [instance, setInstance] = useState("");
  const [pending, setPending] = useState<Pending | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [seconds, setSeconds] = useState(10);
  const [recoveryNotice, setRecoveryNotice] = useState(false);
  const lock = useRef(false);
  const generation = useRef(0);
  const pendingRef = useRef<Pending | null>(null);
  const customized = useRef(false);
  const cancelButton = useRef<HTMLButtonElement>(null);
  const preparedThemes = useRef(new WeakSet<AuroraTheme>());
  pendingRef.current = pending;
  customized.current = theme.id !== DEFAULT_THEME.id || pending !== null;

  // Native snapshots include media bytes. Reuse unchanged documents so a
  // confirmation or association toggle cannot restart/decode a large video.
  const acceptLibrary = useCallback((next: ThemeLibrary, changedId?: string) => {
    setLibrary((current) => ({ ...next, themes: next.themes.map((item) => {
      const saved = item.id !== changedId ? current.themes.find((theme) => theme.id === item.id) : undefined;
      const candidate = saved ?? item;
      // Also tolerate a running older dev host while the WebView hot-reloads.
      return candidate.window && candidate.cursor ? candidate : parseTheme(JSON.stringify(candidate));
    }) }));
  }, []);

  const applyReset = useCallback((next: ThemeLibrary) => {
    generation.current++;
    setPending(null);
    setTheme(DEFAULT_THEME);
    acceptLibrary(next);
  }, [acceptLibrary]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const refresh = () => { void themeCommand<ThemeLibrary>("themes_load").then((next) => {
      if (disposed) return;
      applyReset(next);
      if (next.recovered) setRecoveryNotice(true);
    }).catch((reason) => { if (!disposed) setError(publicError(reason)); }); };
    refresh();
    if (isTauri()) void listen("aurora-theme-reset", refresh).then((stop) => { if (disposed) stop(); else unlisten = stop; });
    else window.addEventListener("aurora-theme-reset", refresh);
    return () => { disposed = true; unlisten?.(); window.removeEventListener("aurora-theme-reset", refresh); };
  }, [applyReset]);

  const reset = useCallback(async () => {
    generation.current++;
    setPending(null);
    setTheme(DEFAULT_THEME);
    setLibrary((current) => ({ ...current, selectedThemeId: null, automatic: false }));
    setError("");
    try { applyReset(await themeCommand<ThemeLibrary>("themes_reset")); }
    catch (reason) { setError(publicError(reason)); throw reason; }
  }, [applyReset]);

  const mediaFailed = useCallback((message: string) => {
    void reset().catch(() => {}).finally(() => setError(message));
  }, [reset]);

  useEffect(() => {
    let disposed = false;
    const beat = () => { void themeCommand("themes_heartbeat", { customized: customized.current && !document.hidden }).catch(() => {
      if (!disposed && customized.current) {
        generation.current++;
        setPending(null);
        setTheme(DEFAULT_THEME);
        setLibrary((current) => ({ ...current, selectedThemeId: null, automatic: false }));
        setError("A supervisão da aparência foi interrompida. O visual padrão foi restaurado.");
      }
    }); };
    beat();
    const timer = setInterval(beat, 1_000);
    document.addEventListener("visibilitychange", beat);
    return () => { disposed = true; clearInterval(timer); document.removeEventListener("visibilitychange", beat); };
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.shiftKey && event.key === "F12") {
        event.preventDefault();
        void reset().catch(() => {});
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [reset]);

  useEffect(() => {
    if (!pending) return;
    const previousFocus = document.activeElement;
    cancelButton.current?.focus();
    const tick = () => {
      const remaining = pending.deadlineMs - Date.now();
      setSeconds(Math.max(0, Math.ceil(remaining / 1_000)));
      if (remaining <= 0) void reset().catch(() => {});
    };
    tick();
    const timer = setInterval(tick, 100);
    return () => {
      clearInterval(timer);
      queueMicrotask(() => { if (previousFocus instanceof HTMLElement && previousFocus.isConnected) previousFocus.focus(); });
    };
  }, [pending, reset]);

  const desired = useMemo(() => {
    if (pending) return pending.theme;
    const id = library.automatic && instance && library.bindings[instance] || library.selectedThemeId;
    return library.themes.find((item) => item.id === id) ?? DEFAULT_THEME;
  }, [library, instance, pending]);

  useEffect(() => {
    let disposed = false;
    const expected = generation.current;
    if (desired === DEFAULT_THEME) { setTheme(DEFAULT_THEME); return; }
    // Pending media has already been prepared before Rust started its deadline.
    if (pending?.theme === desired) { setTheme(desired); return; }
    const prepare = preparedThemes.current.has(desired) ? Promise.resolve() : preloadTheme(desired);
    void prepare.then(async () => {
      if (disposed || expected !== generation.current) return;
      preparedThemes.current.add(desired);
      await themeCommand("themes_heartbeat", { customized: !document.hidden });
      if (!disposed && expected === generation.current) setTheme(desired);
    }).catch((reason) => { if (!disposed && expected === generation.current) mediaFailed(publicError(reason)); });
    return () => { disposed = true; };
  }, [desired, mediaFailed, pending]);

  const action = useCallback(async (run: () => Promise<void>) => {
    if (lock.current || pendingRef.current) throw new Error("Conclua a prévia atual antes de alterar os temas.");
    lock.current = true;
    setBusy(true);
    setError("");
    const expected = generation.current;
    try { await run(); }
    catch (reason) { if (expected === generation.current) setError(publicError(reason)); throw reason; }
    finally { lock.current = false; setBusy(false); }
  }, []);

  const save = useCallback((json: string) => action(async () => {
    const parsed = parseTheme(json);
    const expected = generation.current;
    const next = await themeCommand<ThemeLibrary>("themes_save", { json });
    if (expected === generation.current) acceptLibrary(next, parsed.id);
  }), [action, acceptLibrary]);
  const remove = useCallback((id: string) => action(async () => {
    const expected = generation.current;
    const next = await themeCommand<ThemeLibrary>("themes_delete", { id });
    if (expected === generation.current) acceptLibrary(next);
  }), [action, acceptLibrary]);
  const automatic = useCallback((enabled: boolean) => action(async () => {
    const expected = generation.current;
    const next = await themeCommand<ThemeLibrary>("themes_set_automatic", { enabled });
    if (expected === generation.current) acceptLibrary(next);
  }), [action, acceptLibrary]);
  const unbind = useCallback((instanceId: string) => action(async () => {
    const expected = generation.current;
    const next = await themeCommand<ThemeLibrary>("themes_unbind", { instanceId });
    if (expected === generation.current) acceptLibrary(next);
  }), [action, acceptLibrary]);
  const preview = useCallback((id: string, instanceId: string | null) => action(async () => {
    const candidate = library.themes.find((item) => item.id === id);
    if (!candidate) throw new Error("Salve o tema antes de visualizar.");
    const expected = generation.current;
    if (!preparedThemes.current.has(candidate)) await preloadTheme(candidate);
    if (expected !== generation.current) return;
    preparedThemes.current.add(candidate);
    const ticket = await themeCommand<ThemePreview>("themes_preview", { id, instanceId });
    if (expected !== generation.current) { await themeCommand("themes_reset"); return; }
    setSeconds(10);
    setTheme(candidate);
    setPending({ ...ticket, theme: candidate, instanceId });
  }), [action, library.themes]);

  async function confirm() {
    const current = pendingRef.current;
    if (!current || lock.current) return;
    lock.current = true;
    setBusy(true);
    const expected = generation.current;
    try {
      const next = await themeCommand<ThemeLibrary>("themes_confirm", { token: current.token });
      if (expected === generation.current && pendingRef.current?.token === current.token) {
        setPending(null);
        acceptLibrary(next);
      }
    } catch (reason) {
      if (expected === generation.current) {
        await reset().catch(() => {});
        setError(publicError(reason));
      }
    } finally { lock.current = false; setBusy(false); }
  }

  const value: ThemesContext = { library, theme, busy: busy || !!pending, error, setInstance, save, preview, remove, reset, automatic, unbind, mediaFailed };
  return <Context.Provider value={value}>
    {children}
    {createPortal(<div className="theme-safety-layer">
      {pending && <aside className="theme-confirmation" role="dialog" aria-modal="false" aria-labelledby="theme-confirm-title">
        <div><strong id="theme-confirm-title">Manter esta aparência?</strong><p>Sem confirmação, o Aurora volta ao padrão em <b>{seconds}s</b>.</p></div>
        <div className="theme-safety-actions"><button type="button" disabled={busy || seconds <= 0} onClick={() => { void confirm(); }}>Manter tema</button>
          <button ref={cancelButton} type="button" onClick={() => { void reset().catch(() => {}); }}>Voltar ao padrão</button></div>
      </aside>}
      {!pending && theme.id !== DEFAULT_THEME.id && <button className="theme-emergency" title="Restaurar padrão (Ctrl+Shift+F12)" type="button" onClick={() => { void reset().catch(() => {}); }}>↺ Restaurar aparência</button>}
      {recoveryNotice && <aside className="theme-recovery" role="status"><span>Após uma interrupção, a aparência padrão foi recuperada. Seus temas continuam salvos.</span><button aria-label="Dispensar aviso de recuperação" type="button" onClick={() => setRecoveryNotice(false)}>×</button></aside>}
      {error && <aside className="theme-error-notice" role="alert"><span>{error}</span><button aria-label="Fechar aviso de aparência" type="button" onClick={() => setError("")}>×</button></aside>}
    </div>, document.body)}
  </Context.Provider>;
}

export function useThemes() {
  const context = useContext(Context);
  if (!context) throw new Error("O gerenciador de aparência não foi iniciado.");
  return context;
}
