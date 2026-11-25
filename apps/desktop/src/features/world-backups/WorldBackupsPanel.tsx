import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import "./worldBackups.css";

type Version = { id: string; world: string; createdAt: number; bytes: number; files: number };
type Overview = { globalEnabled: boolean; instanceEnabled: boolean | null; pending: boolean; lastError: string | null; versions: Version[]; storedBytes: number };
const size = (bytes: number) => bytes >= 1024 ** 3 ? (bytes / 1024 ** 3).toLocaleString("pt-BR", { maximumFractionDigits: 2 }) + " GiB"
  : (bytes / 1024 ** 2).toLocaleString("pt-BR", { maximumFractionDigits: 2 }) + " MiB";
const when = (timestamp: number) => new Date(timestamp).toLocaleString("pt-BR");

export function WorldBackupsPanel(props: { instanceId: string; running?: boolean }) {
  return <WorldHistory key={props.instanceId} {...props} />;
}
function WorldHistory({ instanceId, running = false }: { instanceId: string; running?: boolean }) {
  const [data, setData] = useState<Overview>(), [message, setMessage] = useState(""), [error, setError] = useState("");
  const [world, setWorld] = useState(""), [version, setVersion] = useState("");
  const [busy, setBusy] = useState(false), [revision, setRevision] = useState(0);
  const mounted = useRef(false), mutating = useRef(false);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => {
    let active = true, timer: number | undefined;
    async function refresh() {
      try {
        const next = await invoke<Overview>("world_backup_overview", { id: instanceId });
        if (!active) return;
        setData(next);
        if (next.pending && !running && !next.lastError) timer = window.setTimeout(refresh, 4000);
      } catch {
        if (active) setError("Não foi possível consultar os backups. Tente atualizar a lista.");
      }
    }
    void refresh();
    return () => { active = false; window.clearTimeout(timer); };
  }, [instanceId, running, revision]);
  async function run(command: string, args: Record<string, unknown>, describe: (result: string) => string) {
    if (mutating.current) return;
    mutating.current = true; setBusy(true); setError(""); setMessage("");
    try {
      const result = await invoke<string>(command, { id: instanceId, ...args });
      if (mounted.current) setMessage(describe(result));
    } catch (cause) {
      if (mounted.current) setError(typeof cause === "string" ? cause : "Não foi possível concluir a operação.");
    } finally {
      mutating.current = false;
      if (mounted.current) { setBusy(false); setRevision(old => old + 1); }
    }
  }
  const worlds = [...new Set(data?.versions.map(v => v.world))];
  const selectedWorld = worlds.includes(world) ? world : worlds[0];
  const versions = data?.versions.filter(v => v.world === selectedWorld) ?? [];
  const selected = versions.find(v => v.id === version) ?? versions[0];
  return <section className="world-backups-panel" aria-label="Backups dos mundos">
    <header><h3>Backups dos mundos</h3><p>O Aurora guarda os mundos de um jogador desta instância quando o Minecraft fecha.</p></header>
    {data && <>
      <label>Backup automático nesta instância
        <select disabled={busy} value={data.instanceEnabled === null ? "inherit" : String(data.instanceEnabled)}
          onChange={event => void run("world_backup_configure", { global: false, enabled: event.target.value === "inherit" ? null : event.target.value === "true" }, () => "Preferência da instância salva.")}>
          <option value="inherit">Usar padrão global ({data.globalEnabled ? "ativado" : "desativado"})</option>
          <option value="true">Ativado</option><option value="false">Desativado</option>
        </select>
      </label>
      <label><input type="checkbox" checked={data.globalEnabled} disabled={busy}
        onChange={event => void run("world_backup_configure", { global: true, enabled: event.target.checked }, () => "Padrão global salvo.")} /> Ativar por padrão nas instâncias</label>
      {data.lastError ? <p role="alert">{data.lastError}</p> : data.pending && <p role="status">{running ? "O backup será verificado depois que o jogo fechar." : "Verificando o backup pendente…"}</p>}
      {data.lastError && data.pending && <button className="secondary" type="button" disabled={running || busy} onClick={() => void run("world_backup_retry", {}, () => "Verificação concluída.")}>Tentar backup novamente</button>}
      {selected ? <>
        <label>Mundo<select value={selectedWorld} onChange={event => { setWorld(event.target.value); setVersion(""); }}>
          {worlds.map(name => <option key={name} value={name}>{name}</option>)}</select></label>
        <label>Versão salva<select value={selected.id} onChange={event => setVersion(event.target.value)}>
          {versions.map(v => <option key={v.id} value={v.id}>{when(v.createdAt)} · {size(v.bytes)}</option>)}</select></label>
        <p>{selected.files.toLocaleString("pt-BR")} arquivos nesta versão. A instância guarda {size(data.storedBytes)} em blocos únicos.</p>
        <button type="button" disabled={running || busy} onClick={() => void run("world_backup_restore", { version: selected.id }, name => "Mundo restaurado na pasta " + name + ". Abra Um jogador no Minecraft para encontrá-lo.")}>Restaurar como novo mundo</button>
        <small>{running ? "Feche o Minecraft para restaurar." : "A cópia mantém o nome do mundo no Minecraft e recebe outra pasta em saves."}</small>
      </> : <p>Ainda não há versões salvas.</p>}
      <small>Até cinco versões por mundo. Blocos iguais ocupam espaço uma única vez.</small>
    </>}
    {!data && !error && <p role="status">Consultando os backups…</p>}
    <button type="button" className="secondary compact" disabled={busy} onClick={() => { setError(""); setRevision(old => old + 1); }}>Atualizar lista de backups</button>
    {message && <p role="status">{message}</p>}{error && <p role="alert">{error}</p>}
  </section>;
}
