import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { connectDate, connectError, type ConnectStatusSummary } from "./types";

type Candidate = { candidateId: string; target: string; bindKind: "loopback" | "wildcard" };
type StatusRuntime = { minecraftVersion: string; loader: "fabric" | "forge"; loaderVersion: string; statusProtocol: number; evidence: "nativeLaunchProfile" };
type Inventory = { inventoryId: string; instanceId: string; runId: string; expiresAtMs: number; candidates: Candidate[]; runtime: StatusRuntime | null };
type Review = { reviewId: string; digest: string; inventoryId: string; candidate: Candidate; instanceId: string; runId: string; expiresAtMs: number; statusProtocol: number; handshakeHost: "localhost"; handshakePort: number; runtime: StatusRuntime };
export type DiscoveredTarget = { selectionId: string; target: string; instanceId: string; runId: string; summary: ConnectStatusSummary; verifiedBeforeAfter: true; expiresAtMs: number; worldState: "notVerified"; runtime: StatusRuntime };
const statusTargets = [
  ["1.12.2", "forge", "14.23.5.2860", 340], ["1.16.5", "fabric", "0.13.3", 754], ["1.16.5", "forge", "36.2.42", 754],
  ["1.19.2", "fabric", "0.16.14", 760], ["1.19.2", "forge", "43.4.16", 760],
  ["1.20.1", "fabric", "0.16.14", 763], ["1.20.1", "forge", "47.4.23", 763],
  ["1.21.1", "fabric", "0.16.14", 767], ["1.21.1", "forge", "52.1.12", 767],
] as const;
const object = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
const text = (v: unknown): v is string => typeof v === "string" && v.length > 0 && v.length <= 256 && !/[\u0000-\u001f\u007f]/u.test(v);
const integer = (v: unknown): v is number => Number.isSafeInteger(v) && (v as number) >= 0;
const candidate = (v: unknown): v is Candidate => object(v) && text(v.candidateId) && text(v.target) && ["loopback", "wildcard"].includes(v.bindKind as string);
const runtime = (v: unknown): v is StatusRuntime => object(v) && Object.keys(v).length === 5 && v.evidence === "nativeLaunchProfile"
  && statusTargets.some(([mc, loader, version, protocol]) => v.minecraftVersion === mc && v.loader === loader && v.loaderVersion === version && v.statusProtocol === protocol);
const sameRuntime = (a: StatusRuntime, b: StatusRuntime) => a.minecraftVersion === b.minecraftVersion && a.loader === b.loader && a.loaderVersion === b.loaderVersion && a.statusProtocol === b.statusProtocol;
const invalid = (): never => { throw new Error("Invalid discovery projection"); };
export function inventoryFrom(v: unknown): Inventory {
  if (!object(v) || !text(v.inventoryId) || !text(v.instanceId) || !text(v.runId) || !integer(v.expiresAtMs)
    || !Array.isArray(v.candidates) || v.candidates.length > 32 || !v.candidates.every(candidate)
    || (v.runtime !== null && !runtime(v.runtime))) return invalid();
  return v as Inventory;
}
export function discoveryReviewFrom(v: unknown): Review {
  if (!object(v) || !text(v.reviewId) || !text(v.inventoryId) || !text(v.instanceId) || !text(v.runId)
    || typeof v.digest !== "string" || !/^[a-f0-9]{64}$/.test(v.digest) || !candidate(v.candidate)
    || !integer(v.expiresAtMs) || !runtime(v.runtime) || v.statusProtocol !== v.runtime.statusProtocol || v.handshakeHost !== "localhost"
    || !integer(v.handshakePort) || v.handshakePort < 1 || v.handshakePort > 65535) return invalid();
  return v as Review;
}
export function discoveredTargetFrom(v: unknown): DiscoveredTarget {
  if (!object(v) || !text(v.selectionId) || !text(v.target) || !text(v.instanceId) || !text(v.runId)
    || !integer(v.expiresAtMs) || !runtime(v.runtime) || v.verifiedBeforeAfter !== true || v.worldState !== "notVerified" || !object(v.summary)) return invalid();
  const s = v.summary;
  if (s.requestedProtocol !== v.runtime.statusProtocol || !Number.isInteger(s.reportedProtocol)
    || s.protocolMatchesRequested !== (s.requestedProtocol === s.reportedProtocol)
    || ![s.connectMicros, s.statusMicros, s.pingMicros, s.totalMicros].every(integer)
    || ![s.reportedPlayersOnline, s.reportedPlayersMax].every(n => n === null || integer(n))) return invalid();
  return v as DiscoveredTarget;
}

export function DiscoveryPicker({ disabled, onSelect }: { disabled: boolean; onSelect: (target: DiscoveredTarget | undefined) => void }) {
  const [instances, setInstances] = useState<string[]>([]);
  const [inventory, setInventory] = useState<Inventory>();
  const [review, setReview] = useState<Review>();
  const [selected, setSelected] = useState<DiscoveredTarget>();
  const [busy, setBusy] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState("");
  const generation = useRef(0);
  const mounted = useRef(true);
  const blocked = useRef(disabled);
  blocked.current = disabled;
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; generation.current++; }; }, []);
  async function perform(work: (current: () => boolean) => Promise<void>) {
    if (disabled || busy) return;
    const request = ++generation.current;
    const current = () => mounted.current && !blocked.current && request === generation.current;
    setBusy(true); setError(""); setSelected(undefined); onSelect(undefined);
    try { await work(current); }
    catch (cause) { if (current()) { setReview(undefined); setError(connectError(cause)); } }
    finally { if (mounted.current && request === generation.current) setBusy(false); }
  }
  async function loadInstances(current: () => boolean) {
    setInventory(undefined); setReview(undefined);
    const value: unknown = await invoke("list_running_instances");
    if (!Array.isArray(value) || value.length > 64 || !value.every(v => object(v) && text(v.instanceId))) return invalid();
    if (current()) { setInstances([...new Set(value.map(v => v.instanceId as string))]); setLoaded(true); }
  }
  return <section className="connect-discovery" aria-label="Descobrir servidor da instância">
    <p>Abra o mundo para LAN no jogo iniciado pelo Aurora. A busca confere somente portas desse processo; não envia consultas até sua confirmação.</p>
    <button type="button" className="secondary" disabled={disabled || busy} onClick={() => { void perform(loadInstances); }}>Buscar jogos abertos pelo Aurora</button>
    {busy && <p role="status">Conferindo a seleção…</p>}
    {error && <p role="alert">{error}</p>}
    {loaded && instances.length === 0 && <p>Nenhum jogo iniciado por este launcher está aberto. Servidores iniciados fora dele podem usar o endereço manual.</p>}
    {instances.map(id => <div key={id}><button type="button" className="secondary" disabled={disabled || busy} onClick={() => { void perform(async current => {
      setInventory(undefined); setReview(undefined);
      const next = inventoryFrom(await invoke("connect_discover_instance", { instanceId: id }));
      if (next.instanceId !== id) return invalid();
      if (current()) setInventory(next);
    }); }}>Ver portas de {id}</button></div>)}
    {inventory && <div><p>Instância: {inventory.instanceId}. Seleção válida até {connectDate(inventory.expiresAtMs)}.</p>
      {!inventory.runtime && <p>O perfil desta execução não pôde ser confirmado. As portas são somente um inventário; a consulta Minecraft está indisponível.</p>}
      {inventory.candidates.length === 0 && <p>Nenhuma porta local disponível. Abra o mundo para LAN e repita a busca.</p>}
      {inventory.candidates.map(item => <div key={item.candidateId}><button type="button" className="secondary" disabled={disabled || busy || !inventory.runtime} onClick={() => { void perform(async current => {
        setReview(undefined);
        const next = discoveryReviewFrom(await invoke("connect_review_discovered", { inventoryId: inventory.inventoryId, candidateId: item.candidateId }));
        if (next.inventoryId !== inventory.inventoryId || next.instanceId !== inventory.instanceId || next.runId !== inventory.runId
          || next.candidate.candidateId !== item.candidateId || next.candidate.target !== item.target
          || !inventory.runtime || !sameRuntime(next.runtime, inventory.runtime)) return invalid();
        if (current()) setReview(next);
      }); }}>Revisar {item.target}</button></div>)}
    </div>}
    {review && !selected && <div className="connect-review"><p>Consultar Minecraft {review.runtime.minecraftVersion} ({review.runtime.loader} {review.runtime.loaderVersion}, protocolo {review.statusProtocol}) na porta <code>{review.candidate.target}</code> da instância {review.instanceId}.</p>
      <p>A resposta confirma disponibilidade, sem comprovar entrada no mundo. O jogo pode estar escutando também em outras interfaces; esta ação não muda sua configuração.</p>
      <button type="button" disabled={disabled || busy} onClick={() => { void perform(async current => {
        const next = discoveredTargetFrom(await invoke("connect_confirm_discovered_status", { reviewId: review.reviewId, digest: review.digest }));
        if (next.target !== review.candidate.target || next.instanceId !== review.instanceId || next.runId !== review.runId
          || !sameRuntime(next.runtime, review.runtime)) return invalid();
        if (current()) { setSelected(next); setReview(undefined); onSelect(next); }
      }); }}>Autorizar consulta e selecionar porta</button>
    </div>}
    {selected && <p role="status">Servidor respondeu em <code>{selected.target}</code>. Protocolo {selected.summary.reportedProtocol}, {selected.summary.protocolMatchesRequested ? "correspondente" : "diferente"} ao solicitado ({selected.summary.requestedProtocol}). Prepare o anfitrião até {connectDate(selected.expiresAtMs)}; o processo e a porta serão conferidos novamente.</p>}
  </section>;
}
