import { useEffect, useMemo, useRef, useState } from "react";
import { isPlayerIdentifier, LAST_CATALOG_PAGE_NUMBER, PlayerSkinsClient, type CatalogPage, type PlayerSuggestion } from "../../lib/playerSkins";
import "./onlineSkins.css";
import { SkinFace } from "./SkinFace";

type Selection = { name: string; skinUrl: string; model: "classic" | "slim"; contentHash: string };
export function OnlineSkinSearch({ workerUrl, onSelect, disabled = false }: { workerUrl: string; onSelect: (skin: Selection) => void; disabled?: boolean }) {
  const client = useMemo(() => new PlayerSkinsClient(workerUrl), [workerUrl]);
  const [mode, setMode] = useState<"players" | "catalog">("catalog");
  const [query, setQuery] = useState("");
  const [players, setPlayers] = useState<PlayerSuggestion[]>([]);
  const [catalog, setCatalog] = useState<CatalogPage>();
  const [request, setRequest] = useState({ page: 0, from: 0 });
  const [pageInput, setPageInput] = useState("1");
  const [retry, setRetry] = useState(false);
  const [status, setStatus] = useState("");
  const [loading, setLoading] = useState(false);
  const selection = useRef<AbortController | null>(null);
  const search = useRef<AbortController | null>(null);
  const currentPage = catalog?.page ?? 0;
  const requestedNumber = Number(pageInput);
  const validPage = /^\d+$/.test(pageInput) && requestedNumber >= 1 && requestedNumber <= LAST_CATALOG_PAGE_NUMBER;
  function requestPage(page: number) {
    setRequest({ page, from: currentPage });
  }
  function resetSearch(nextMode: typeof mode, nextQuery: string) {
    setMode(nextMode);
    setQuery(nextQuery);
    setCatalog(undefined);
    setPlayers([]);
    setRequest({ page: 0, from: 0 });
    setPageInput("1");
  }
  useEffect(() => {
    const controller = new AbortController();
    search.current = controller;
    selection.current?.abort();
    if (disabled) { setLoading(false); return () => controller.abort(); }
    setStatus(""); setRetry(false);
    if (mode === "catalog" && query.trim() === "#") {
      setStatus("Digite uma tag depois de #.");
      setLoading(false);
      return () => controller.abort();
    }
    if (mode === "players" && !/^[A-Za-z0-9_]{3,16}$/.test(query.trim())) {
      setLoading(false);
      return () => controller.abort();
    }
    setLoading(true);
    const timer = window.setTimeout(() => {
      if (controller.signal.aborted) return;
      const load = mode === "players"
        ? client.autocompleteSkinNames(query.trim(), controller.signal).then((value) => { if (!controller.signal.aborted) setPlayers(value); })
        : client.getCatalog(query.trim(), request.page, controller.signal).then((value) => {
          if (controller.signal.aborted) return;
          if (!value.sources.includes("mcstat") && value.entries.length === 0) {
            throw new Error("O catálogo online está indisponível no momento.");
          }
          if (value.page > 0 && !value.hasMore && value.entries.length === 0) {
            setStatus(`A página ${value.page + 1} está vazia. Escolha outro número.`);
            setPageInput(String(request.from + 1));
            return;
          }
          setCatalog(value);
          setPageInput(String(value.page + 1));
          if (value.stale) {
            setStatus(`Página ${value.page + 1} salva na última consulta. O catálogo online está temporariamente indisponível.`);
            setRetry(true);
          }
        });
      void load.catch((error) => {
        if (controller.signal.aborted) return;
        setStatus(error instanceof Error ? error.message : "Falha na busca.");
        setRetry(mode === "catalog");
      })
        .finally(() => { if (!controller.signal.aborted) setLoading(false); });
    }, 250);
    return () => { clearTimeout(timer); controller.abort(); };
  }, [client, mode, query, request, disabled]);
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
      <button type="button" disabled={disabled} className={mode === "catalog" ? "selected compact" : "secondary compact"} aria-pressed={mode === "catalog"} onClick={() => resetSearch("catalog", "")}>Catálogo de skins</button>
      <button type="button" disabled={disabled} className={mode === "players" ? "selected compact" : "secondary compact"} aria-pressed={mode === "players"} onClick={() => resetSearch("players", "")}>Jogadores</button>
    </div>
    <form className="username-search" onSubmit={(event) => { event.preventDefault(); if (mode === "players" && query.trim()) void choosePlayer(query.trim()); }}>
      <input aria-label={mode === "players" ? "Nome ou UUID Minecraft" : "Tema da skin"} value={query} maxLength={80} disabled={disabled}
        onChange={(event) => resetSearch(mode, event.target.value)} placeholder={mode === "players" ? "Nome ou UUID Minecraft" : "Tema, nome ou #tag"} />
      {mode === "players" && <button type="submit" disabled={disabled || !isPlayerIdentifier(query)}>Buscar perfil</button>}
    </form>
    {mode === "catalog" && <nav className="online-skin-pagination" aria-label="Paginação do catálogo">
      <form onSubmit={(event) => { event.preventDefault(); if (!disabled && !loading && validPage) requestPage(requestedNumber - 1); }}>
        <label>Página<input type="number" inputMode="numeric" min="1" max={LAST_CATALOG_PAGE_NUMBER} step="1" value={pageInput} aria-label="Ir para página"
          disabled={disabled || loading} onChange={(event) => setPageInput(event.target.value)} /></label>
        <button type="submit" className="secondary compact" disabled={disabled || loading || !validPage}>Ir</button>
      </form>
      <div>
        <button className="secondary compact" type="button" disabled={disabled || loading || currentPage === 0} onClick={() => requestPage(currentPage - 1)}>Anterior</button>
        <button className="secondary compact" type="button" disabled={disabled || loading || !catalog?.hasMore || currentPage + 1 >= LAST_CATALOG_PAGE_NUMBER} onClick={() => requestPage(currentPage + 1)}>Próxima</button>
      </div>
    </nav>}
    <div className="online-skin-results" aria-label={mode === "catalog" ? `Skins da página ${currentPage + 1}` : "Jogadores encontrados"} aria-busy={loading}>
      {players.map((player) => <button type="button" className="secondary online-player-result" key={`${player.uuid ?? player.name}:${player.name}`} disabled={disabled}
        onClick={() => void choosePlayer(player.uuid || player.name)}><strong>{player.name}</strong><small>{player.metadata.current === false ? "Nome anterior · " : ""}{player.source}</small></button>)}
      {catalog?.entries.map((entry) => <button className="secondary online-catalog-result" disabled={disabled || loading} type="button" key={entry.id} onClick={() => {
        onSelect({ name: entry.name, model: entry.model, skinUrl: client.resolveCatalogUrl(entry), contentHash: entry.hash });
        setStatus(`${entry.name} carregada na prévia.`);
      }}><SkinFace url={client.resolveCatalogUrl(entry)} name={entry.name} /><span><strong>{entry.name}</strong><small>{entry.author}</small></span></button>)}
    </div>
    <p className="muted" role="status" aria-live="polite">{loading ? "Buscando skins…" : status || (catalog?.entries.length === 0 ? catalog.hasMore ? "Nenhuma skin nesta página. Avance para a próxima." : "Nenhuma skin encontrada." : mode === "players" && players.length === 0
      ? "Busque pelo nome ou UUID. Sugestões aparecem a partir de 3 caracteres."
      : catalog ? `Página ${currentPage + 1} · ${catalog.entries.length} ${catalog.entries.length === 1 ? "skin" : "skins"}` : "")}</p>
    {retry && <button className="secondary compact" type="button" disabled={disabled || loading} onClick={() => requestPage(request.page)}>Tentar novamente</button>}
  </section>;
}
