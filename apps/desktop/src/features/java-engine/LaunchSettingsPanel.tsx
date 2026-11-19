import { invoke } from "@tauri-apps/api/core";
import { useEffect, useId, useRef, useState } from "react";
import type { FormEvent } from "react";
import { defaultLaunchSettings } from "./types";
import type { GlobalMemoryOverview, LaunchSettings, LaunchSettingsOverview } from "./types";
import "./java-engine.css";

type Props = { instanceId: string; busy?: boolean; running?: boolean };
type Draft = {
  memoryMode: LaunchSettings["memoryMode"];
  minMemoryMb: string;
  maxMemoryMb: string;
  gcMode: LaunchSettings["gcMode"];
  args: string;
};
type State = {
  instanceId: string;
  phase: "loading" | "ready" | "error";
  overview?: LaunchSettingsOverview;
  draft?: Draft;
  replacingUnreadable?: boolean;
};

function draftFrom(settings: LaunchSettings): Draft {
  return {
    memoryMode: settings.memoryMode,
    minMemoryMb: settings.minMemoryMb == null ? "" : String(settings.minMemoryMb),
    maxMemoryMb: String(settings.maxMemoryMb),
    gcMode: settings.gcMode,
    args: settings.jvmArgs.join("\n"),
  };
}

function settingsFrom(draft: Draft): LaunchSettings | string {
  const manualMaximum = Number(draft.maxMemoryMb);
  const validManualMaximum = Number.isInteger(manualMaximum) && manualMaximum >= 512 && manualMaximum <= 262144;
  // Automatic allocation never uses this retained manual value. Canonicalize an
  // unfinished manual edit so a disabled field cannot block automatic mode.
  const maxMemoryMb = draft.memoryMode !== "manual" && !validManualMaximum
    ? defaultLaunchSettings().maxMemoryMb : manualMaximum;
  const minMemoryMb = draft.memoryMode !== "manual" || !draft.minMemoryMb.trim()
    ? null : Number(draft.minMemoryMb);
  if (!Number.isInteger(maxMemoryMb) || maxMemoryMb < 512 || maxMemoryMb > 262144) {
    return "A memória máxima deve ser um número inteiro entre 512 e 262144 MB.";
  }
  if (minMemoryMb != null && (!Number.isInteger(minMemoryMb) || minMemoryMb < 512 || minMemoryMb > maxMemoryMb)) {
    return "A memória inicial deve ficar entre 512 MB e a memória máxima, ou ficar vazia.";
  }
  return {
    schemaVersion: 1,
    memoryMode: draft.memoryMode,
    minMemoryMb,
    maxMemoryMb,
    gcMode: draft.gcMode,
    jvmArgs: draft.args.split(/\r?\n/).map((arg) => arg.trim()).filter(Boolean),
  };
}

export function LaunchSettingsPanel({ instanceId, busy = false, running = false }: Props) {
  const id = useId();
  const generation = useRef(0);
  const context = useRef(instanceId);
  context.current = instanceId;
  const [state, setState] = useState<State>();
  const [reload, setReload] = useState(0);
  const [savingFor, setSavingFor] = useState<string>();
  const [message, setMessage] = useState<{ instanceId: string; text: string; error: boolean; reload?: boolean }>();
  const current = state?.instanceId === instanceId ? state : undefined;
  const draft = current?.draft;
  const overview = current?.overview;
  const saving = savingFor === instanceId;
  const blocked = busy || running || saving || current?.phase !== "ready";
  const dirty = !!draft && !!overview && (current?.replacingUnreadable || JSON.stringify(draft) !== JSON.stringify(draftFrom(overview.settings)));
  const visibleMessage = message?.instanceId === instanceId ? message : undefined;

  useEffect(() => {
    const request = ++generation.current;
    let disposed = false;
    setState({ instanceId, phase: "loading" });
    setMessage(undefined);
    setSavingFor(undefined);
    if (!instanceId) {
      setState({ instanceId, phase: "error" });
      return;
    }
    void invoke<LaunchSettingsOverview>("read_launch_settings", { id: instanceId }).then((result) => {
      if (!disposed && request === generation.current && context.current === instanceId) {
        setState({ instanceId, phase: "ready", overview: result, draft: draftFrom(result.settings) });
      }
    }).catch(() => {
      if (!disposed && request === generation.current && context.current === instanceId) setState({ instanceId, phase: "error" });
    });
    return () => { disposed = true; generation.current += 1; };
  }, [instanceId, reload]);

  function edit(update: Partial<Draft>) {
    if (blocked) return;
    setState((value) => value?.instanceId === instanceId && value.draft
      ? { ...value, draft: { ...value.draft, ...update } } : value);
    setMessage(undefined);
  }

  async function save(event: FormEvent) {
    event.preventDefault();
    if (blocked || !dirty || !draft) return;
    const settings = settingsFrom(draft);
    if (typeof settings === "string") {
      setMessage({ instanceId, text: settings, error: true });
      return;
    }
    const request = ++generation.current;
    setSavingFor(instanceId);
    setMessage(undefined);
    try {
      if (overview?.settings.memoryRevision) settings.memoryRevision = overview.settings.memoryRevision;
      const result = await invoke<LaunchSettingsOverview>("save_launch_settings", { id: instanceId, settings });
      if (request !== generation.current || context.current !== instanceId) return;
      setState({ instanceId, phase: "ready", overview: result, draft: draftFrom(result.settings) });
      setMessage({ instanceId, text: "Configurações salvas para a próxima inicialização.", error: false });
    } catch (error) {
      if (request === generation.current && context.current === instanceId) {
        const changed = error === "global_memory_changed";
        setMessage({ instanceId, text: changed ? "A memória foi aplicada a todos enquanto o editor estava aberto. Recarregue os valores antes de salvar." : "Não foi possível salvar. Revise os valores e os argumentos permitidos e tente novamente.", error: true, reload: changed });
      }
    } finally {
      if (request === generation.current && context.current === instanceId) setSavingFor(undefined);
    }
  }

  function cancel() {
    if (blocked || !overview) return;
    setState(current?.replacingUnreadable ? { instanceId, phase: "error" }
      : { instanceId, phase: "ready", overview, draft: draftFrom(overview.settings) });
    setMessage(undefined);
  }

  async function recoverDefaults() {
    if (busy || running || saving) return;
    const request = ++generation.current;
    setSavingFor(instanceId);
    try {
      const global = await invoke<GlobalMemoryOverview>("read_global_memory");
      if (request !== generation.current || context.current !== instanceId) return;
      const settings = defaultLaunchSettings(global.maxMemoryMb);
      settings.memoryRevision = global.memoryRevision;
      setState({ instanceId, phase: "ready", overview: { settings, globalMaxMemoryMb: global.maxMemoryMb, totalMemoryMb: global.totalMemoryMb, modCount: 0, recommendedMemoryMb: null }, draft: draftFrom(settings), replacingUnreadable: true });
      setMessage({ instanceId, text: "Padrões carregados nos campos. Salve para substituir a configuração local que não pôde ser lida.", error: false });
    } catch {
      if (request === generation.current && context.current === instanceId) setMessage({ instanceId, text: "Revise a memória padrão em Java & engine antes de restaurar este modpack.", error: true });
    } finally {
      if (request === generation.current && context.current === instanceId) setSavingFor(undefined);
    }
  }

  return <section className="java-engine-settings" data-theme-surface="java-memory-settings" aria-labelledby={`${id}-heading`} aria-busy={saving || current?.phase === "loading"}>
    <header className="java-engine-heading">
      <h3 id={`${id}-heading`}>Memória deste modpack</h3>
    </header>
    {!instanceId ? <p role="status">Selecione uma instância para configurar a inicialização.</p>
      : !current || current.phase === "loading" ? <p role="status">Lendo as configurações da instância…</p>
      : current.phase === "error" ? <div className="java-engine-load-error" role="alert"><p>Não foi possível ler as configurações desta instância.</p><div className="java-engine-actions"><button className="secondary" type="button" disabled={busy || running || saving} onClick={() => setReload((value) => value + 1)}>Tentar novamente</button><button className="secondary" type="button" disabled={busy || running || saving} onClick={recoverDefaults}>Restaurar padrões</button></div></div>
      : draft && overview && <form onSubmit={(event) => void save(event)} noValidate>
        {running && <p className="java-engine-note" role="status">Feche o Minecraft desta instância para alterar a inicialização.</p>}
        <fieldset disabled={blocked}>
          <legend className="java-memory-legend">Memória RAM</legend>
          <div className="java-engine-fields">
            <label htmlFor={`${id}-memory-mode`}>Modo de alocação<select id={`${id}-memory-mode`} value={draft.memoryMode} onChange={(event) => {
              const memoryMode = event.target.value as Draft["memoryMode"];
              edit(memoryMode === "launcher" ? { memoryMode, maxMemoryMb: String(overview.globalMaxMemoryMb), minMemoryMb: "" } : { memoryMode });
            }}><option value="launcher">Padrão do launcher</option><option value="manual">Manual</option><option value="automatic">Automático</option></select></label>
            <label htmlFor={`${id}-maximum`}>Memória máxima (MB)<input id={`${id}-maximum`} type="number" min="512" max="262144" step="1" disabled={draft.memoryMode !== "manual"} value={draft.memoryMode === "automatic" ? overview.recommendedMemoryMb ?? "" : draft.maxMemoryMb} onChange={(event) => edit({ maxMemoryMb: event.target.value })} /></label>
            <label htmlFor={`${id}-minimum`}>Memória inicial (MB, opcional)<input id={`${id}-minimum`} type="number" min="512" max={draft.maxMemoryMb} step="1" placeholder="Padrão do Java" disabled={draft.memoryMode !== "manual"} value={draft.memoryMode === "manual" ? draft.minMemoryMb : ""} onChange={(event) => edit({ minMemoryMb: event.target.value })} /></label>
          </div>
          <p className="java-engine-hint">{current.replacingUnreadable ? "Inventário indisponível até a próxima leitura." : <>RAM total: {overview.totalMemoryMb == null ? "não disponível" : `${overview.totalMemoryMb} MB`} · {overview.modCount} JARs de mods · Recomendação: {overview.recommendedMemoryMb == null ? "não disponível" : `${overview.recommendedMemoryMb} MB`}.</>}</p>
          {draft.memoryMode === "automatic" && <p className="java-engine-note">O Aurora recalcula o limite ao iniciar, conforme a RAM total e os mods, preservando 2048 MB para o sistema. A memória inicial fica a cargo do Java. A memória livre pode variar entre execuções.</p>}
        </fieldset>
        <details className="java-advanced">
          <summary>Opções avançadas da JVM</summary>
          <fieldset disabled={blocked}>
            <label htmlFor={`${id}-collector`}>Coletor de memória<select id={`${id}-collector`} value={draft.gcMode} onChange={(event) => edit({ gcMode: event.target.value as Draft["gcMode"] })}><option value="runtime">Padrão do Java</option><option value="recommended">Recomendado para a versão do Java</option></select></label>
            <p className="java-engine-hint">A recomendação usa G1 no Java 17 ou superior e mantém o padrão nas versões anteriores. Não há garantia de ganho de desempenho.</p>
          </fieldset>
          <fieldset disabled={blocked}>
            <legend>Argumentos JVM personalizados</legend>
            <label htmlFor={`${id}-arguments`}>Um argumento por linha<textarea id={`${id}-arguments`} rows={4} spellCheck={false} value={draft.args} onChange={(event) => edit({ args: event.target.value })} placeholder={"-XX:MaxGCPauseMillis=200\n-Xss1M"} aria-describedby={`${id}-args-help`} /></label>
            <p className="java-engine-hint" id={`${id}-args-help`}>Cada linha vira um argumento literal. Não use aspas de shell ou comandos de terminal.</p>
            <details><summary>Argumentos permitidos e limites</summary><p>São aceitas opções de desempenho validadas pelo Aurora, como <code>-XX:+UseG1GC</code>, <code>-XX:MaxGCPauseMillis=200</code>, <code>-Xss1M</code> e <code>-ea</code>, e propriedades restritas como <code>-Duser.language=pt</code>, <code>-Duser.country=BR</code> e <code>-Dfile.encoding=UTF-8</code>. Configure a RAM nos campos acima. Use o coletor padrão ao escolher um coletor pelos argumentos.</p><p>As demais propriedades <code>-D</code>, agentes, arquivos de argumentos, classpath, comandos antes/depois do jogo e alterações de identidade ou IPC são bloqueados. O Java ainda pode recusar opções incompatíveis com sua versão.</p></details>
          </fieldset>
        </details>
        <div className="java-engine-actions">
          <button type="submit" disabled={blocked || !dirty}>{saving ? "Salvando…" : "Salvar configurações"}</button>
          <button className="secondary" type="button" disabled={blocked || !dirty} onClick={cancel}>Cancelar alterações</button>
          <button className="secondary" type="button" disabled={blocked} onClick={() => edit(draftFrom(defaultLaunchSettings(overview.globalMaxMemoryMb)))}>Restaurar padrões</button>
        </div>
      </form>}
    {visibleMessage && <p className="java-engine-message" role={visibleMessage.error ? "alert" : "status"}>{visibleMessage.text}{visibleMessage.reload && <button type="button" className="secondary" onClick={() => setReload(value => value + 1)}>Recarregar valores</button>}</p>}
  </section>;
}
