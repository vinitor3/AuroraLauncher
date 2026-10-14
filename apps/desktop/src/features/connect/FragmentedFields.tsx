import type { ConnectRole } from "./types";
import { FRAGMENTED_LIFETIME_SECONDS, FRAGMENTED_UDP_BIND, fragmentedUdpErrorText, fragmentedUdpStateText,
  type FragmentedGuestUdp, type FragmentedUdpReview } from "./fragmented-types";

type FieldsProps = {
  role: ConnectRole; disabled: boolean; udpTarget: string; lifetimeSeconds: string;
  separateSession?: boolean;
  onUdpTargetChange: (value: string) => void; onLifetimeChange: (value: string) => void;
};

/** Controlled fields only: rendering never invokes native code or writes files. */
export function FragmentedFields(props: FieldsProps) {
  return <>
    {props.role === "host" && <>
      <label>Serviço UDP neste computador<input required type="text" maxLength={128} value={props.udpTarget} disabled={props.disabled}
        onChange={event => props.onUdpTargetChange(event.target.value)} autoComplete="off" spellCheck={false} />
        <small>Informe o endereço loopback e a porta que você autoriza compartilhar. A porta não comprova qual mod está em execução.</small></label>
      {!props.separateSession && <label>Prazo do convite e da conexão, em segundos<input required type="number" min={1} max={FRAGMENTED_LIFETIME_SECONDS} step={1}
        value={props.lifetimeSeconds} disabled={props.disabled} onChange={event => props.onLifetimeChange(event.target.value)} />
        <small>Este perfil tem um único vencimento, contado desde a emissão e sem renovação.</small></label>}
    </>}
    <p>A ponte UDP do convidado usa {FRAGMENTED_UDP_BIND}. Se ela não puder abrir, a conexão do jogo permanece independente; outra porta não será escolhida automaticamente.</p>
    <p>Somente transporte experimental. Anúncio e configuração dos mods exigem plano e aprovação separados; áudio não testado. Este perfil não configura mundos LAN nem modo misto.</p>
  </>;
}

/** Place inside the parent's reviewed definition list, before its consent. */
export function FragmentedReviewFields({ review }: { review: FragmentedUdpReview }) {
  return <>
    {review.udpTarget !== null && <><dt>Destino UDP autorizado</dt><dd><code>{review.udpTarget}</code></dd></>}
    <dt>Ponte UDP do convidado</dt><dd><code>{review.localUdpBind}</code></dd>
    <dt>Datagrama original aprovado</dt><dd>{review.approvedOriginalDatagramBytes} bytes · fragmentação limitada</dd>
    <dt>Retorno UDP autorizado</dt><dd>{review.udpReturnPolicy.bytesPerSecond} bytes por segundo, incluindo os cabeçalhos da ponte, e {review.udpReturnPolicy.datagramsPerSecond} fragmentos por segundo; orçamento compartilhado pelo serviço</dd>
    <dt>Configuração dos mods</dt><dd>Não verificada · exige plano, diff, autorização e recuperação separados</dd>
    <dt>Áudio</dt><dd>Não testado · limite de transporte não homologa voz</dd>
  </>;
}

/** UDP failure does not change the parent's TCP/server status or actions. */
export function FragmentedStatusFields({ udp }: { udp?: FragmentedGuestUdp }) {
  return <>
    <dt>Transporte UDP</dt><dd>{udp ? fragmentedUdpStateText[udp.udpState] : "Serviço UDP previsto · a ponte local pertence ao convidado"}</dd>
    {udp?.udpState === "available" && <>
      <dt>Endereço UDP local</dt><dd><code>{udp.udpAddress}</code></dd>
      <dt>Limite inicial UDP</dt><dd>{udp.udpOriginalCapacity} bytes na primeira associação; o caminho pode reduzir esse limite</dd>
    </>}
    {udp?.udpErrorCode && <><dt>Falha UDP observada</dt><dd>{fragmentedUdpErrorText(udp.udpErrorCode)}</dd></>}
    <dt>Anúncio e configuração dos mods</dt><dd>Não verificados</dd><dt>Áudio</dt><dd>Não testado</dd>
  </>;
}
