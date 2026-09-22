import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { type ReactNode, useEffect, useRef, useState } from "react";

function WindowTitlebar() {
  const [maximized, setMaximized] = useState(false);
  const [error, setError] = useState("");
  const actionPending = useRef(false);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    let disposed = false;
    let stopListening: (() => void) | undefined;
    const refresh = async () => {
      try {
        const current = await appWindow.isMaximized();
        if (!disposed) setMaximized(current);
      } catch {
        // Window actions still work if a state read is temporarily unavailable.
      }
    };
    void refresh();
    void appWindow.onResized(() => { void refresh(); }).then((unlisten) => {
      if (disposed) unlisten();
      else stopListening = unlisten;
    }).catch(() => {});
    return () => {
      disposed = true;
      stopListening?.();
    };
  }, []);

  async function control(action: "minimize" | "maximize" | "close") {
    if (actionPending.current) return;
    actionPending.current = true;
    setError("");
    try {
      const appWindow = getCurrentWindow();
      if (action === "minimize") await appWindow.minimize();
      else if (action === "close") await appWindow.close();
      else {
        await appWindow.toggleMaximize();
        setMaximized(await appWindow.isMaximized());
      }
    } catch {
      setError("Não foi possível alterar a janela. Tente novamente.");
    } finally {
      actionPending.current = false;
    }
  }

  return (
    <>
      <header className="window-titlebar" aria-label="Janela do Aurora">
        <div className="window-titlebar-drag" data-tauri-drag-region>
          <span className="window-titlebar-emblem" aria-hidden="true">✦</span>
          <span className="window-titlebar-name">Aurora <span>Smart Launcher</span></span>
        </div>
        <div className="window-titlebar-controls">
          <button className="window-control" type="button" title="Minimizar" aria-label="Minimizar janela" onClick={() => { void control("minimize"); }}>
            <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M5 10.5h10" /></svg>
          </button>
          <button className="window-control" type="button" title={maximized ? "Restaurar tamanho" : "Maximizar"} aria-label={maximized ? "Restaurar tamanho da janela" : "Maximizar janela"} onClick={() => { void control("maximize"); }}>
            <svg viewBox="0 0 20 20" aria-hidden="true">
              {maximized ? <><path d="M7.5 6V4.5h8v8H14" /><rect x="4.5" y="7.5" width="8" height="8" rx="1" /></> : <rect x="5" y="5" width="10" height="10" rx="1.5" />}
            </svg>
          </button>
          <button className="window-control window-control-close" type="button" title="Fechar janela e continuar na bandeja. Para sair, use “Fechar o launcher” no ícone do Aurora." aria-label="Fechar janela e continuar na bandeja" onClick={() => { void control("close"); }}>
            <svg viewBox="0 0 20 20" aria-hidden="true"><path d="m6 6 8 8M14 6l-8 8" /></svg>
          </button>
        </div>
      </header>
      {error && <div className="window-control-error" role="alert">{error}</div>}
    </>
  );
}

export function DesktopWindow({ children }: { children: ReactNode }) {
  if (!isTauri()) return <>{children}</>;
  return <div className="desktop-window"><WindowTitlebar />{children}</div>;
}
