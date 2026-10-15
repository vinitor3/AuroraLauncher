import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";

const idPattern = /^aurora-test-[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/;
const uuidPattern = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/;
const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
/** Presentation only. Native preparation independently verifies provenance,
 * physical identity, runtime, modules and that no process owns this copy. */
export function readyCopyIdsFrom(value: unknown): string[] {
  if (!Array.isArray(value) || value.length > 64) throw new Error("Invalid diagnostic copy list");
  const seen = new Set<string>();
  const ready: string[] = [];
  for (const item of value) {
    if (!object(item) || typeof item.id !== "string" || !idPattern.test(item.id) || seen.has(item.id)
      || !["ready", "creating", "failed", "deleting"].includes(item.state as string)) throw new Error("Invalid diagnostic copy list");
    seen.add(item.id);
    if (item.state !== "ready") continue;
    if (typeof item.instanceUuid !== "string" || !uuidPattern.test(item.instanceUuid)
      || typeof item.originUuid !== "string" || !uuidPattern.test(item.originUuid)) throw new Error("Invalid ready copy identity");
    ready.push(item.id);
  }
  return ready;
}

type Props = { sourceInstanceId: string; disabled: boolean; selectedId: string; onSelect: (id: string) => void };
export function ReadyCopyPicker({ sourceInstanceId, disabled, selectedId, onSelect }: Props) {
  const [ids, setIds] = useState<string[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const epoch = useRef(0);
  const mounted = useRef(false);
  const current = useRef({ sourceInstanceId, disabled, onSelect });
  current.current = { sourceInstanceId, disabled, onSelect };
  useEffect(() => {
    mounted.current = true; epoch.current++; setIds([]); setLoaded(false); setBusy(false); setError("");
    current.current.onSelect("");
    return () => { mounted.current = false; epoch.current++; };
  }, [sourceInstanceId]);
  async function load() {
    if (disabled || busy || !sourceInstanceId) return;
    const request = ++epoch.current;
    const source = sourceInstanceId;
    setBusy(true); setError(""); setIds([]); setLoaded(false); onSelect("");
    const live = () => mounted.current && request === epoch.current && current.current.sourceInstanceId === source && !current.current.disabled;
    try {
      const next = readyCopyIdsFrom(await invoke<unknown>("diagnostic_lab_list", { id: source }));
      if (live()) { setIds(next); setLoaded(true); }
    } catch {
      if (live()) setError("Não foi possível conferir as cópias de teste. Atualize a lista antes de selecionar.");
    } finally { if (mounted.current && request === epoch.current) setBusy(false); }
  }
  return <section className="connect-fields" aria-label="Selecionar cópia pronta">
    <button className="secondary" type="button" disabled={disabled || busy || !sourceInstanceId} onClick={() => { void load(); }}>Listar cópias prontas</button>
    {busy && <p role="status">Conferindo as cópias disponíveis…</p>}
    {error && <p role="alert">{error}</p>}
    {loaded && ids.length === 0 && <p>Nenhuma cópia pronta disponível para esta instância. Prepare e feche uma cópia pela seção Cópias de teste antes de continuar.</p>}
    {ids.length > 0 && <label>Cópia de teste para abrir<select value={selectedId} disabled={disabled || busy} onChange={event => {
      const id = event.target.value; if (id === "" || ids.includes(id)) onSelect(id);
    }}><option value="">Selecione uma cópia</option>{ids.map(id => <option value={id} key={id}>{id}</option>)}</select></label>}
    <small>A seleção não instala arquivos. Minecraft, Fabric, Java, Core e Skins serão conferidos na revisão.</small>
  </section>;
}
