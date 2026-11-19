import { useId, useState } from "react";
import type { SocialSessionDraft } from "../../shared/social";
import { DiscoveryPicker, type DiscoveredTarget } from "../connect/DiscoveryPicker";
import { ThemeSurface } from "../themes/ThemeSurface";
import { socialGameTarget } from "./connect-types";

type Props = {
  nickname: string;
  instanceNames: Record<string, string>;
  disabled: boolean;
  allowApproval: boolean;
  now: number;
  onPublish: (draft: SocialSessionDraft, source: DiscoveredTarget) => void;
};

export function LanWorldShare({ nickname, instanceNames, disabled, allowApproval, now, onPublish }: Props) {
  const heading = useId();
  const [source, setSource] = useState<DiscoveredTarget>();
  const [title, setTitle] = useState(() => `Mundo de ${nickname}`);
  const [visibility, setVisibility] = useState<"friends" | "public">("friends");
  const [approval, setApproval] = useState(false);
  const supported = !!source && socialGameTarget(source.target) && source.summary.protocolMatchesRequested;
  const capacity = source?.summary.reportedPlayersMax;
  const online = source?.summary.reportedPlayersOnline ?? 0;
  const guestSlots = capacity == null ? 8 : Math.min(8, Math.max(0, capacity - online));
  const name = source && (instanceNames[source.instanceId] || "Minecraft Java").slice(0, 64);
  return <ThemeSurface className="lan-world-share theme-protected" surfaceId="social-lan-world" labelledBy={heading}>
    <h3 id={heading}>Compartilhar meu mundo LAN</h3>
    <DiscoveryPicker instanceNames={instanceNames} disabled={disabled} onSelect={setSource} />
    {source && (!supported ? <p role="alert">Escolha uma porta IPv4 do mundo com a versão de Minecraft correspondente.</p>
      : guestSlots === 0 ? <p role="alert">O mundo está lotado. Verifique novamente quando houver uma vaga.</p>
      : source.expiresAtMs <= now ? <p role="alert">Use Encontrar meu mundo LAN para verificar a seleção novamente.</p>
      : <form onSubmit={event => {
        event.preventDefault();
        if (disabled || !name || !title.trim() || source.expiresAtMs <= Date.now()) return;
        onPublish({ title: title.trim(), modpackName: name, minecraftVersion: source.runtime.minecraftVersion,
          loader: source.runtime.loader, players: 0, maxPlayers: guestSlots,
          imageUrl: null, visibility, joinPolicy: allowApproval && approval ? "approval" : "open" }, source);
      }}>
        <p><strong>{name}</strong> · Minecraft {source.runtime.minecraftVersion} · {source.runtime.loader}</p>
        <div className="social-fields">
          <label>Nome do mundo<input required value={title} maxLength={64} disabled={disabled} onChange={event => setTitle(event.target.value)} /></label>
          <label>Compartilhar com<select value={visibility} disabled={disabled} onChange={event => setVisibility(event.target.value === "public" ? "public" : "friends")}>
            <option value="friends">Meus amigos</option><option value="public">Toda a comunidade</option>
          </select></label>
        </div>
        {allowApproval && <label className="social-consent"><input type="checkbox" checked={approval} disabled={disabled} onChange={event => setApproval(event.target.checked)} />Aprovar cada pedido de entrada</label>}
        <p>O nome, o modpack e a versão ficarão visíveis para o público escolhido. Você confirma a conexão na próxima tela.</p>
        <button disabled={disabled || !title.trim()}>{visibility === "public" ? "Publicar para a comunidade" : "Publicar para amigos"}</button>
      </form>)}
  </ThemeSurface>;
}
