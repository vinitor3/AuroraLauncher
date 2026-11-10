import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { readyGameError, type ReadyGameRuntime } from "./types";

const idPattern = /^aurora-test-[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/;
const uuidPattern = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/;
const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
/** Presentation only. Native preparation independently verifies provenance,
 * physical identity, runtime, modules and that no process owns this copy. */
type ReadyCopy = { id: string; originInstanceId?: string; java?: string };
function readyCopiesFrom(value: unknown): ReadyCopy[] {
  if (!Array.isArray(value) || value.length > 64) throw new Error("Invalid diagnostic copy list");
  const seen = new Set<string>();
  const ready: ReadyCopy[] = [];
  for (const item of value) {
    if (!object(item) || typeof item.id !== "string" || !idPattern.test(item.id) || seen.has(item.id)
      || !["ready", "creating", "failed", "deleting"].includes(item.state as string)) throw new Error("Invalid diagnostic copy list");
    seen.add(item.id);
    if (item.state !== "ready") continue;
    if (typeof item.instanceUuid !== "string" || !uuidPattern.test(item.instanceUuid)
      || typeof item.originUuid !== "string" || !uuidPattern.test(item.originUuid)) throw new Error("Invalid ready copy identity");
    const java = item.runtimeJavaExecutable;
    if (java !== undefined && (typeof java !== "string" || java.length > 4096
      || /[\u0000-\u001f]/u.test(java)
      || !java.replaceAll("\\", "/").includes(`/instances/${item.id}/installers/aurora-managed/runtimes/`)
      || !java.endsWith("java.exe"))) throw new Error("Invalid managed Java");
    ready.push({ id: item.id, originInstanceId: typeof item.originInstanceId === "string" ? item.originInstanceId : undefined, java });
  }
  return ready;
}
export function readyCopyIdsFrom(value: unknown): string[] {
  return readyCopiesFrom(value).map(copy => copy.id);
}

const profileId = (runtime: ReadyGameRuntime) => runtime.loader === "fabric"
  ? `fabric-loader-${runtime.loaderVersion}-${runtime.minecraftVersion}`
  : `${runtime.minecraftVersion}-forge-${runtime.loaderVersion}`;
type Props = { sourceInstanceId: string; contextKey?: string; connectOperationId?: string; requiredProfile?: string;
  voiceProfile?: "svc" | "plasmo";
  disabled: boolean; selectedId: string; onSelect: (id: string, java?: string) => void; runtimes?: readonly ReadyGameRuntime[] };
export function ReadyCopyPicker({ sourceInstanceId, contextKey, connectOperationId, requiredProfile, voiceProfile, disabled, selectedId, onSelect, runtimes = [] }: Props) {
  const [copies, setCopies] = useState<ReadyCopy[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [target, setTarget] = useState("");
  const [approved, setApproved] = useState(false);
  const [install, setInstall] = useState<string>();
  const [cancelRequested, setCancelRequested] = useState(false);
  const epoch = useRef(0);
  const mounted = useRef(false);
  const current = useRef({ sourceInstanceId, connectOperationId, requiredProfile, disabled, onSelect });
  current.current = { sourceInstanceId, connectOperationId, requiredProfile, disabled, onSelect };
  useEffect(() => {
    mounted.current = true; epoch.current++; setCopies([]); setLoaded(false); setBusy(false); setError(""); setTarget(""); setApproved(false);
    current.current.onSelect("");
    return () => { mounted.current = false; epoch.current++; };
  }, [sourceInstanceId, contextKey, connectOperationId, requiredProfile]);
  async function load() {
    if (disabled || busy || install || !sourceInstanceId) return;
    const request = ++epoch.current;
    const source = sourceInstanceId;
    setBusy(true); setError(""); setCopies([]); setLoaded(false); onSelect("");
    const live = () => mounted.current && request === epoch.current && current.current.sourceInstanceId === source
      && current.current.connectOperationId === connectOperationId && current.current.requiredProfile === requiredProfile
      && !current.current.disabled;
    try {
      const next = readyCopiesFrom(await invoke<unknown>("diagnostic_lab_list", { id: source }));
      if (live()) { setCopies(next); setLoaded(true); }
    } catch {
      if (live()) setError("Não foi possível conferir as cópias de teste. Atualize a lista antes de selecionar.");
    } finally { if (mounted.current && request === epoch.current) setBusy(false); }
  }
  const offered = requiredProfile ? runtimes.filter(runtime => profileId(runtime) === requiredProfile) : runtimes;
  const selectedProfile = requiredProfile && offered.length ? requiredProfile : target;
  const targetRuntime = offered.find(runtime => profileId(runtime) === selectedProfile);
  const originalSelected = !sourceInstanceId.startsWith("aurora-test-");
  async function prepareRuntime() {
    if (disabled || busy || install || !targetRuntime || !approved || !sourceInstanceId || !connectOperationId || !originalSelected) return;
    const operationId = crypto.randomUUID();
    const source = sourceInstanceId;
    const request = ++epoch.current;
    setInstall(operationId); setApproved(false); setCancelRequested(false); setError("");
    const live = () => mounted.current && request === epoch.current && current.current.sourceInstanceId === source
      && current.current.connectOperationId === connectOperationId && current.current.requiredProfile === requiredProfile;
    try {
      const value = await invoke<unknown>("connect_install_ready_copy", {
        request: { operationId, connectOperationId, sourceInstanceId: source, profileId: profileId(targetRuntime), approved: true },
      });
      const [copy] = readyCopiesFrom([value]);
      if (!copy?.java || copy.originInstanceId !== source) throw new Error("Wrong prepared copy");
      if (live()) {
        setCopies(previous => [...previous.filter(item => item.id !== copy.id), copy]);
        setLoaded(true);
        if (!current.current.disabled) current.current.onSelect(copy.id, copy.java);
      }
    } catch (cause) {
      if (live()) setError(readyGameError(cause));
    } finally {
      if (mounted.current) { setInstall(undefined); setCancelRequested(false); }
    }
  }
  async function cancelInstall() {
    if (!install || cancelRequested) return;
    setCancelRequested(true);
    try { await invoke("connect_cancel_ready_copy_install", { operationId: install }); }
    catch (cause) { if (mounted.current) { setError(readyGameError(cause)); setCancelRequested(false); } }
  }
  return <section className="connect-fields connect-ready-copy" aria-label="Selecionar cópia pronta">
    {voiceProfile && <p>Esta sessão usa <strong>{voiceProfile === "svc" ? "Simple Voice Chat" : "Plasmo Voice"}</strong>. Escolha uma cópia compatível ou prepare uma nova. Cópias com outro mod de voz não são convertidas; a revisão confere a compatibilidade antes de abrir.</p>}
    <button className="secondary" type="button" disabled={disabled || busy || !!install || !sourceInstanceId} onClick={() => { void load(); }}>Listar cópias prontas</button>
    {busy && <p role="status">Conferindo as cópias disponíveis…</p>}
    {error && <p role="alert">{error}</p>}
    {loaded && copies.length === 0 && <p>Nenhuma cópia pronta disponível para esta instância. Prepare uma cópia de runtime abaixo ou use Cópias de teste para partir do modpack existente.</p>}
    {copies.length > 0 && <label>Cópia de teste para abrir<select value={selectedId} disabled={disabled || busy || !!install} onChange={event => {
      const id = event.target.value;
      const copy = copies.find(item => item.id === id);
      if (copy || id === "") onSelect(id, copy?.java);
    }}><option value="">Selecione uma cópia</option>{copies.map(copy => <option value={copy.id} key={copy.id}>{copy.id}{copy.java ? " · runtime oficial" : ""}</option>)}</select></label>}
    <small>A seleção não instala arquivos. Minecraft, loader, Java, Core e Skins serão conferidos na revisão.</small>
    {requiredProfile && runtimes.length > 0 && !offered.length && <p>Esta versão do Connect não oferece o perfil assinado da sessão.</p>}
    {offered.length > 0 && !originalSelected && !install && <p>Selecione a instância de origem para preparar outra cópia de runtime.</p>}
    {(offered.length > 0 && originalSelected || !!install) && <details>
      <summary>Preparar uma nova cópia de runtime</summary>
      <fieldset disabled={disabled || busy || !!install || !originalSelected}>
        <label>Perfil oficial da nova cópia<select value={selectedProfile} disabled={!!requiredProfile} onChange={event => { setTarget(event.target.value); setApproved(false); }}>
          <option value="">Selecione o perfil</option>
          {offered.map(runtime => <option key={profileId(runtime)} value={profileId(runtime)}>
            Minecraft {runtime.minecraftVersion} · {runtime.loader === "forge" ? "Forge" : "Fabric"} {runtime.loaderVersion}
          </option>)}
        </select></label>
        {targetRuntime && <>
          <p>A cópia será criada com Java {targetRuntime.javaMajor}, Core {targetRuntime.coreVersion}, Skins {targetRuntime.skinsVersion} e os arquivos oficiais do perfil selecionado. Mundos, mods e configurações do modpack ficam na origem.</p>
          {targetRuntime.loader === "forge" && <p>O preparo executa o instalador oficial do Forge com o Java da cópia.</p>}
          <label className="connect-consent"><input type="checkbox" checked={approved} onChange={event => setApproved(event.target.checked)} /><span>Autorizo baixar e instalar o perfil acima em uma nova cópia de teste.</span></label>
          <button type="button" disabled={!approved || !connectOperationId} onClick={() => { void prepareRuntime(); }}>Criar cópia com este runtime</button>
        </>}
      </fieldset>
      {install && <><p role="status">Preparando a cópia de runtime…</p><button type="button" disabled={cancelRequested} onClick={() => { void cancelInstall(); }}>Cancelar instalação da cópia</button></>}
    </details>}
  </section>;
}
