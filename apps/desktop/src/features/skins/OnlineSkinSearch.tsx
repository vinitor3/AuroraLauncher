import { useEffect, useMemo, useRef, useState } from "react";
import { isPlayerIdentifier, PlayerSkinsClient, type CatalogPage, type PlayerSuggestion } from "../../lib/playerSkins";
import "./onlineSkins.css";
import { SkinFace } from "./SkinFace";

type Selection = { name: string; skinUrl: string; model: "classic" | "slim"; contentHash: string };
export function OnlineSkinSearch({ workerUrl, onSelect, disabled = false }: { workerUrl: string; onSelect: (skin: Selection) => void; disabled?: boolean }) {
  const client = useMemo(() => new PlayerSkinsClient(workerUrl), [workerUrl]);
  const [mode, setMode] = useState<"players" | "catalog">("catalog");
  const [query, setQuery] = useState("");
  const [players, setPlayers] = useState<PlayerSuggestion[]>([]);
  const [catalog, setCatalog] = useState<CatalogPage>();
  const [page, setPage] = useState(0);
  const [status, setStatus] = useState("");
  const [loading, setLoading] = useState(false);
  const selection = useRef<AbortController | null>(null);
  const search = useRef<AbortController | null>(null);
  useEffect(() => {
    const controller = new AbortController();
    search.current = controller;
    selection.current?.abort();
    if (disabled) { setLoading(false); return () => controller.abort(); }
    setPlayers([]); setCatalog(undefined); setStatus(""); setLoading(false);
    if (mode === "players" && !/^[A-Za-z0-9_]{3,16}$/.test(query.trim())) return () => controller.abort();
    const timer = window.setTimeout(() => {
      if (controller.signal.aborted) return;
      setLoading(true);
      const load = mode === "players"
        ? client.autocompleteSkinNames(query.trim(), controller.signal).then((value) => { if (!controller.signal.aborted) setPlayers(value); })
        : client.getCatalog(query.trim(), page, controller.signal).then((value) => { if (!controller.signal.aborted) setCatalog(value); });
      void load.catch((error) => { if (!controller.signal.aborted) setStatus(error instanceof Error ? error.message : "Falha na busca."); })
        .finally(() => { if (!controller.signal.aborted) setLoading(false); });
    }, 250);
    return () => { clearTimeout(timer); controller.abort(); };
  }, [client, mode, query, page, disabled]);
  useEffect(() => () => selection.current?.abort(), []);
  async function choosePlayer(identifier: string) {
    if (disabled || !isPlayerIdentifier(identifier)) return;
    search.current?.abort();
    selection.current?.abort();
    const controller = new AbortController(); selection.current = controller;
    setLoading(true); setStatus("Carregando skin…");
    try {
      const result = await client.getPlayerProfile(identifier, controller.signal);
      if (controller.signal.aborted) return;
      onSelect(result);
      setStatus(result.stale ? "Última skin disponível carregada na prévia." : `Skin de ${result.name} carregada na prévia.`);
    } catch (error) { if (!controller.signal.aborted) setStatus(error instanceof Error ? error.message : "Falha ao carregar skin."); }
    finally { if (!controller.signal.aborted) setLoading(false); }
  }
  return <section className="online-skin-search" aria-label="Buscar skins online">
    <div className="import-tabs">
      <button type="button" disabled={disabled} className={mode === "catalog" ? "selected compact" : "secondary compact"} aria-pressed={mode === "catalog"} onClick={() => { setMode("catalog"); setPage(0); setQuery(""); }}>Catálogo de skins</button>
      <button type="button" disabled={disabled} className={mode === "players" ? "selected compact" : "secondary compact"} aria-pressed={mode === "players"} onClick={() => { setMode("players"); setPage(0); setQuery(""); }}>Jogadores</button>
    </div>
    <form className="username-search" onSubmit={(event) => { event.preventDefault(); if (mode === "players" && query.trim()) void choosePlayer(query.trim()); }}>
      <input aria-label={mode === "players" ? "Nome ou UUID Minecraft" : "Tema da skin"} value={query} maxLength={80} disabled={disabled}
        onChange={(event) => { setQuery(event.target.value); setPage(0); }} placeholder={mode === "players" ? "Nome (mínimo 3 letras) ou UUID" : "Tema, nome ou #tag, como #ninja"} />
      {mode === "players" && <button type="submit" disabled={disabled || !isPlayerIdentifier(query)}>Buscar perfil</button>}
    </form>
    <div className="online-skin-results" aria-label="Resultados da busca">
      {players.map((player) => <button type="button" className="secondary online-player-result" key={`${player.uuid ?? player.name}:${player.name}`} disabled={disabled}
        onClick={() => void choosePlayer(player.uuid || player.name)}><strong>{player.name}</strong><small>{player.metadata.current === false ? "Nome anterior · " : ""}{player.source}</small></button>)}
      {catalog?.entries.map((entry) => <button className="secondary online-catalog-result" disabled={disabled || loading} type="button" key={entry.id} onClick={() => {
        onSelect({ name: entry.name, model: entry.model, skinUrl: client.resolveCatalogUrl(entry), contentHash: entry.hash });
        setStatus(`${entry.name} carregada na prévia.`);
      }}><SkinFace url={client.resolveCatalogUrl(entry)} name={entry.name} /><span><strong>{entry.name}</strong><small>{entry.author}</small></span></button>)}
    </div>
    {mode === "catalog" && <div className="online-skin-pagination"><button className="secondary compact" type="button" disabled={disabled || loading || page === 0} onClick={() => setPage(page - 1)}>Anterior</button><span>Página {page + 1}</span><button className="secondary compact" type="button" disabled={disabled || loading || !catalog?.hasMore} onClick={() => setPage(page + 1)}>Próxima</button></div>}
    <p className="muted" role="status" aria-live="polite">{loading ? "Buscando skins…" : status || (catalog?.entries.length === 0 ? "Nenhuma skin encontrada." : mode === "players" && players.length === 0
      ? "Digite o nome da conta Minecraft Java e clique em Buscar perfil. As sugestões aparecem a partir de 3 caracteres."
      : "Selecione uma skin e clique em Aplicar skin. Não é preciso abrir o Minecraft.")}</p>
  </section>;
}
