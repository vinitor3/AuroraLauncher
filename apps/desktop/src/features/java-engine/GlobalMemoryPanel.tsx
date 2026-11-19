import { invoke } from "@tauri-apps/api/core";
import { useEffect, useId, useState } from "react";
import type { FormEvent } from "react";
import { ThemeSurface } from "../themes/ThemeSurface";

import type { GlobalMemoryOverview } from "./types";

export function GlobalMemoryPanel({ busy }: { busy: boolean }) {
  const id = useId();
  const [overview, setOverview] = useState<GlobalMemoryOverview>();
  const [amount, setAmount] = useState("");
  const [includeCustom, setIncludeCustom] = useState(false);
  const [saving, setSaving] = useState(false);
  const [loadError, setLoadError] = useState(false);
  const [reload, setReload] = useState(0);
  const [message, setMessage] = useState<{ error: boolean; text: string }>();
  const limit = Math.min(262144, overview?.totalMemoryMb ?? 262144);
  const blocked = busy || saving;

  useEffect(() => {
    let active = true;
    setLoadError(false);
    void invoke<GlobalMemoryOverview>("read_global_memory").then(result => {
      if (active) {
        setOverview(result);
        setAmount(String(result.maxMemoryMb));
      }
    }).catch(() => { if (active) setLoadError(true); });
    return () => { active = false; };
  }, [reload]);

  function changeAmount(value: string) {
    setAmount(value);
    setMessage(undefined);
  }

  async function save(event: FormEvent) {
    event.preventDefault();
    if (blocked || !overview) return;
    const value = Number(amount);
    if (!Number.isInteger(value) || value < 512 || value > limit) {
      setMessage({ error: true, text: `Escolha um valor inteiro entre 512 e ${limit} MB.` });
      return;
    }
    setSaving(true);
    setMessage(undefined);
    try {
      const result = await invoke<GlobalMemoryOverview>("save_global_memory", { maxMemoryMb: value, replaceOverrides: includeCustom });
      setOverview(result);
      setAmount(String(result.maxMemoryMb));
      setMessage({ error: false, text: includeCustom ? "Memória aplicada a todos os modpacks para a próxima inicialização." : "Memória padrão salva para a próxima inicialização." });
      setIncludeCustom(false);
    } catch {
      setMessage({ error: true, text: "Não foi possível salvar a memória. Tente novamente." });
    } finally {
      setSaving(false);
    }
  }

  return <ThemeSurface surfaceId="java-memory-settings" labelledBy={id} className="global-memory">
    <header><h2 id={id}>Memória dos modpacks</h2>
      <p>Defina a RAM padrão. Para personalizar um modpack, abra <strong>Editar → Memória</strong>.</p>
    </header>
    {overview ? <form onSubmit={event => void save(event)} noValidate>
      <div className="global-memory-values">
        <label htmlFor={`${id}-maximum`}>Memória máxima (MB)
          <input id={`${id}-maximum`} type="number" min={512} max={limit} step="1" disabled={blocked} value={amount} onChange={event => changeAmount(event.target.value)} />
        </label>
        <div className="global-memory-presets" role="group" aria-label="Valores de memória">
          {[2, 4, 6, 8].filter(gb => gb * 1024 <= limit).map(gb => <button className="secondary compact" type="button" aria-pressed={Number(amount) === gb * 1024} disabled={blocked} key={gb} onClick={() => changeAmount(String(gb * 1024))}>{gb} GB</button>)}
        </div>
        {overview.totalMemoryMb != null && <small>{(overview.totalMemoryMb / 1024).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} GB de RAM no computador</small>}
      </div>
      <div className="global-memory-actions">
        <label><input type="checkbox" checked={includeCustom} disabled={blocked} onChange={event => { setIncludeCustom(event.target.checked); setMessage(undefined); }} /> Incluir modpacks com memória personalizada</label>
        <button className="compact" type="submit" disabled={blocked || (Number(amount) === overview.maxMemoryMb && !includeCustom)}>{saving ? "Salvando…" : includeCustom ? "Aplicar a todos" : "Salvar padrão"}</button>
      </div>
    </form> : loadError ? <div role="alert"><p>Não foi possível ler a memória padrão.</p><button className="secondary compact" type="button" onClick={() => setReload(value => value + 1)}>Tentar novamente</button> <button className="secondary compact" type="button" onClick={() => { setOverview({ maxMemoryMb: 4096, totalMemoryMb: null }); setAmount("4096"); setIncludeCustom(true); setMessage({ error: false, text: "Confira o valor e clique em Aplicar a todos para substituir a configuração ilegível." }); }}>Restaurar padrão</button></div> : <p role="status">Lendo memória padrão…</p>}
    {message && <p className="global-memory-feedback" role={message.error ? "alert" : "status"}>{message.text}</p>}
  </ThemeSurface>;
}
