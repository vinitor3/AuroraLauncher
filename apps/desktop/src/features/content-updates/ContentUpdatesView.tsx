import {
  currentVersionLabel,
  modpackCurrentVersionLabel,
  modpackTargetVersionLabel,
  targetVersionLabel,
  type ContentUpdateFilter,
  type ContentUpdateItem,
  type ContentUpdateScanView,
  type ModpackUpdate,
  type UpdateContentType,
} from "./types";

type ContentUpdatesViewProps = {
  contentType: UpdateContentType;
  scan?: ContentUpdateScanView;
  totalAvailable: number;
  selectedFilenames: string[];
  filter: ContentUpdateFilter;
  scanning: boolean;
  applying: boolean;
  running: boolean;
  lastBackupAvailable: boolean;
  modpackUpdate?: ModpackUpdate | null;
  modpackScanError?: string;
  onFilterChange: (filter: ContentUpdateFilter) => void;
  onToggleSelection: (filename: string) => void;
  onSelectAvailable: (items: ContentUpdateItem[]) => void;
  onCheckAgain: () => void;
  onReviewItems: (items: ContentUpdateItem[]) => void;
  onReviewModpack: (modpack: ModpackUpdate) => void;
  onReviewRollback: () => void;
};

const FILTERS: Array<{ value: ContentUpdateFilter; label: string }> = [
  { value: "all", label: "Todos" },
  { value: "updateAvailable", label: "Com atualização" },
  { value: "upToDate", label: "Atualizados" },
  { value: "blocked", label: "Bloqueados" },
  { value: "unmanaged", label: "Não identificados" },
  { value: "auroraManaged", label: "Do Aurora" },
];

const STATUS_COPY: Record<ContentUpdateItem["status"], { label: string; icon: string }> = {
  updateAvailable: { label: "Atualização disponível", icon: "↑" },
  upToDate: { label: "Atualizado", icon: "✓" },
  unmanaged: { label: "Não identificado", icon: "?" },
  blocked: { label: "Atualização bloqueada", icon: "!" },
  auroraManaged: { label: "Gerenciado pelo Aurora", icon: "✦" },
};

function formatCheckedAt(value?: string) {
  if (!value) return "Ainda não verificado";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Verificação concluída";
  return `Verificado às ${new Intl.DateTimeFormat("pt-BR", { hour: "2-digit", minute: "2-digit" }).format(date)}`;
}

function itemTitle(item: ContentUpdateItem) {
  return item.title?.trim() || item.filename;
}

function providerLabel(item: ContentUpdateItem) {
  if (item.provider === "curseforge") return "CurseForge";
  if (item.provider === "modrinth") return "Modrinth";
  return item.projectId ? "Catálogo verificado" : "Arquivo local";
}

export function ContentUpdatesView({
  contentType,
  scan,
  totalAvailable,
  selectedFilenames,
  filter,
  scanning,
  applying,
  running,
  lastBackupAvailable,
  modpackUpdate,
  modpackScanError,
  onFilterChange,
  onToggleSelection,
  onSelectAvailable,
  onCheckAgain,
  onReviewItems,
  onReviewModpack,
  onReviewRollback,
}: ContentUpdatesViewProps) {
  const items = scan?.items ?? [];
  const available = items.filter((item) => item.status === "updateAvailable");
  const upToDate = items.filter((item) => item.status === "upToDate");
  const attention = items.filter((item) => item.status === "blocked" || item.status === "unmanaged");
  const visible = filter === "all" ? items : items.filter((item) => item.status === filter);
  const selectedItems = available.filter((item) => selectedFilenames.includes(item.filename));
  const selectedProvider = selectedItems[0]?.provider ?? (selectedItems.length ? "modrinth" : undefined);
  const availableGroups = (["modrinth", "curseforge"] as const)
    .map((provider) => ({ provider, items: available.filter((item) => (item.provider ?? "modrinth") === provider) }))
    .filter((group) => group.items.length > 0);
  const typeLabel = contentType === "mod" ? "mods" : contentType === "shader" ? "shaders" : "pacotes de recursos";
  const showModpackUpdate = Boolean(modpackUpdate && (modpackUpdate.source || modpackUpdate.projectId));
  const totalIncludingModpack = totalAvailable + (showModpackUpdate && modpackUpdate?.status === "updateAvailable" ? 1 : 0);
  const hasAttention = attention.length > 0 || Boolean(showModpackUpdate && (modpackUpdate?.status === "blocked" || modpackUpdate?.status === "unmanaged"));

  return (
    <div className="updates-view">
      {running && (
        <div aria-live="polite" className="updates-running-warning" role="status">
          <span aria-hidden="true">■</span>
          <div><strong>Esta instância está rodando</strong><p>Feche o Minecraft antes de aplicar ou desfazer atualizações. A verificação continua disponível.</p></div>
        </div>
      )}

      {showModpackUpdate && modpackUpdate && (
        <section className={`modpack-update-callout ${modpackUpdate.status}`} aria-label="Atualização do modpack">
          <span className="modpack-update-icon" aria-hidden="true">◈</span>
          <div>
            <p className="eyebrow">MODPACK{modpackUpdate.source ? ` · ${modpackUpdate.source === "curseforge" ? "CURSEFORGE" : "MODRINTH"}` : ""}</p>
            <strong>{modpackUpdate.name || (modpackUpdate.projectId ? `Modpack ${modpackUpdate.projectId}` : "Modpack desta instância")}</strong>
            {modpackUpdate.status === "updateAvailable" ? (
              <small>{modpackCurrentVersionLabel(modpackUpdate)} <b aria-hidden="true">→</b> {modpackTargetVersionLabel(modpackUpdate)}</small>
            ) : (
              <small>{modpackUpdate.reason || (modpackUpdate.status === "upToDate" ? "O modpack está atualizado." : "Não há atualização segura disponível.")}</small>
            )}
          </div>
          <span className={`update-state-badge ${modpackUpdate.status}`}>
            {modpackUpdate.reasonCode === "manualDownloadRequired" ? "Download oficial necessário" : modpackUpdate.status === "updateAvailable" ? "Nova versão" : modpackUpdate.status === "upToDate" ? "Atualizado" : modpackUpdate.status === "blocked" ? "Bloqueado" : "Não gerenciado"}
          </span>
          {modpackUpdate.status === "updateAvailable" && (
            <button className="compact" disabled={applying || running} onClick={() => onReviewModpack(modpackUpdate)} type="button">Revisar atualização</button>
          )}
        </section>
      )}
      {modpackScanError && <p className="updates-inline-error">Modpack: {modpackScanError}</p>}

      <section className="updates-summary-card" aria-label="Resumo das atualizações">
        <div className="updates-summary-heading">
          <div><p className="eyebrow">CONTEÚDO DA INSTÂNCIA</p><h3>{totalIncludingModpack > 0 ? `${totalIncludingModpack} ${totalIncludingModpack === 1 ? "atualização encontrada" : "atualizações encontradas"}` : hasAttention ? "Verificação concluída" : "Tudo sob controle"}</h3><small>{scanning ? "Verificando versões compatíveis…" : formatCheckedAt(scan?.scannedAt)}</small></div>
          <div className="updates-summary-actions">
            {lastBackupAvailable && <button className="secondary compact" disabled={applying || running} onClick={onReviewRollback} type="button">↶ Desfazer última atualização</button>}
            <button className="secondary compact" disabled={scanning || applying} onClick={onCheckAgain} type="button">{scanning ? "Verificando…" : "↻ Verificar novamente"}</button>
          </div>
        </div>
        <div className="update-metrics" aria-live="polite">
          <span className="available"><b>{available.length}</b><small>prontas em {typeLabel}</small></span>
          <span><b>{upToDate.length}</b><small>atualizados</small></span>
          <span className={attention.length ? "attention" : ""}><b>{attention.length}</b><small>precisam de atenção</small></span>
        </div>
      </section>
      {scan?.warning && <p className="updates-inline-warning" role="status">{scan.warning}</p>}

      <div className="updates-toolbar">
        <div className="update-filter-pills" aria-label="Filtrar estado da atualização" role="group">
          {FILTERS.map((entry) => {
            const count = entry.value === "all" ? items.length : items.filter((item) => item.status === entry.value).length;
            return <button aria-pressed={filter === entry.value} className={filter === entry.value ? "active" : ""} key={entry.value} onClick={() => onFilterChange(entry.value)} type="button">{entry.label}<span>{count}</span></button>;
          })}
        </div>
        {availableGroups.length > 0 && <div className="update-provider-selection">{availableGroups.map((group) => {
          const selected = group.items.every((item) => selectedFilenames.includes(item.filename));
          const providerName = group.provider === "curseforge" ? "CurseForge" : "Modrinth";
          return <button className="secondary compact" disabled={applying} key={group.provider} onClick={() => onSelectAvailable(selected ? [] : group.items)} type="button">{selected ? "Limpar seleção" : availableGroups.length > 1 ? `Selecionar ${providerName} (${group.items.length})` : "Selecionar disponíveis"}</button>;
        })}</div>}
      </div>

      {scan?.error ? (
        <div className="updates-empty error" role="alert"><span>!</span><strong>Não foi possível verificar {typeLabel}</strong><p>{scan.error}</p><button onClick={onCheckAgain} type="button">Tentar novamente</button></div>
      ) : scanning && !scan ? (
        <div aria-live="polite" className="updates-loading" role="status"><span className="update-spinner" /><strong>Comparando versões compatíveis</strong><p>O Aurora está identificando os arquivos sem modificar a instância.</p></div>
      ) : items.length === 0 ? (
        <div className="updates-empty"><span>◇</span><strong>Nenhum {typeLabel} para verificar</strong><p>Adicione conteúdo pelo catálogo e volte aqui para acompanhar as versões.</p></div>
      ) : visible.length === 0 ? (
        <div className="updates-empty compact"><span>⌕</span><strong>Nada neste filtro</strong><p>Escolha outro estado para ver os demais arquivos.</p></div>
      ) : (
        <div className="update-item-list">
          {visible.map((item) => {
            const status = item.reasonCode === "manualDownloadRequired"
              ? { label: "Download oficial necessário", icon: "↗" }
              : STATUS_COPY[item.status];
            const canUpdate = item.status === "updateAvailable";
            const selected = selectedFilenames.includes(item.filename);
            const itemProvider = item.provider ?? "modrinth";
            const selectionLocked = Boolean(selectedProvider && selectedProvider !== itemProvider);
            return (
              <article className={`update-item-row ${item.status} ${selected ? "selected" : ""}`} key={`${contentType}-${item.filename}`}>
                <input aria-label={`Selecionar atualização de ${itemTitle(item)}`} checked={selected} className="content-select" disabled={!canUpdate || applying || selectionLocked} onChange={() => onToggleSelection(item.filename)} title={selectionLocked ? "Atualize cada catálogo em uma etapa separada" : undefined} type="checkbox" />
                <span className="update-item-artwork">
                  <span aria-hidden="true">{contentType === "mod" ? "◆" : contentType === "shader" ? "✦" : "▧"}</span>
                  {item.iconUrl && <img alt="" onError={(event) => { event.currentTarget.style.display = "none"; }} src={item.iconUrl} />}
                </span>
                <div className="update-item-copy">
                  <strong>{itemTitle(item)}</strong>
                  <small title={item.filename}>{providerLabel(item)} · {item.enabled ? "Ativo" : "Pausado"}</small>
                  {canUpdate ? <p><span>{currentVersionLabel(item)}</span><b aria-hidden="true">→</b><span>{targetVersionLabel(item)}</span></p> : <p>{item.reason || (item.status === "upToDate" ? `Versão ${currentVersionLabel(item)}` : status.label)}</p>}
                </div>
                <span className={`update-state-badge ${item.status}`} title={item.reason}><i aria-hidden="true">{status.icon}</i>{status.label}</span>
                {canUpdate && <button className="compact" disabled={applying || running} onClick={() => onReviewItems([item])} type="button">Atualizar</button>}
              </article>
            );
          })}
        </div>
      )}

      {selectedItems.length > 0 && (
        <div aria-live="polite" className="update-selection-bar">
          <span><strong>{selectedItems.length}</strong> selecionado(s)</span>
          <button aria-label="Limpar seleção de atualizações" className="secondary compact" disabled={applying} onClick={() => onSelectAvailable([])} type="button">Limpar</button>
          <button disabled={applying || running} onClick={() => onReviewItems(selectedItems)} type="button">Revisar e atualizar</button>
        </div>
      )}
    </div>
  );
}
