import { invoke, isTauri } from "@tauri-apps/api/core";
import { useRef, useState } from "react";

/** Requests the native tray exit path, including its resource/voice barriers. */
export function ExitLauncherButton({ disabled }: { disabled: boolean }) {
  const inFlight = useRef(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  if (!isTauri()) return null;
  async function requestExit() {
    if (disabled || inFlight.current) return;
    inFlight.current = true; setPending(true); setError("");
    try { await invoke("launcher_request_exit"); }
    catch { setError("Não foi possível solicitar o fechamento. Tente novamente."); }
    finally { inFlight.current = false; setPending(false); }
  }
  return <>
    <button className="text-button sidebar-action" disabled={disabled || pending} type="button" onClick={() => void requestExit()}>
      {pending ? "Solicitando fechamento…" : "Fechar o launcher"}
    </button>
    {error && <small role="alert">{error}</small>}
  </>;
}
