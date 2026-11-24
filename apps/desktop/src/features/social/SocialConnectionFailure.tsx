import { SupportContextButton } from "../support/SupportContextButton";
import { socialConnectError, socialFailureGateText, socialFailureStageText, type SocialConnectProjection } from "./connect-types";

/** Presents the existing native observation without requesting or inferring new evidence. */
export function SocialConnectionFailure({ errorCode, social, className = "social-notice" }: {
  errorCode?: string | null;
  social?: Pick<SocialConnectProjection, "failureStage" | "failureObservation">;
  className?: string;
}) {
  if (!errorCode) return null;
  const observation = social?.failureObservation;
  return <>
    <p className={className} role="alert">
      {social?.failureStage && <><strong>{socialFailureStageText[social.failureStage]}.</strong>{" "}</>}
      {socialConnectError(errorCode)}
    </p>
    <SupportContextButton source="Servidor" context={[social?.failureStage && socialFailureStageText[social.failureStage], socialConnectError(errorCode)].filter(Boolean).join(". ")} />
    {observation && <details className={className}>
      <summary>Detalhes da interrupção</summary>
      <p>Verificação interrompida: {socialFailureGateText[observation.gate]}.</p>
      <ul>
        {observation.bootstrapRoundtripMs !== null && <li>Abertura do serviço: {observation.bootstrapRoundtripMs} ms.</li>}
        {observation.bootstrapUncertaintyMs !== null && <li>Incerteza do horário confirmado: {observation.bootstrapUncertaintyMs} ms.</li>}
        {observation.hostPollElapsedMs !== null && <li>Última consulta de autorização: {observation.hostPollElapsedMs} ms.</li>}
        {observation.hostPollAgeMs !== null && <li>Idade da última autorização confirmada: {observation.hostPollAgeMs} ms.</li>}
        <li>Prazo de sessão ainda restante: {Math.ceil(observation.sessionRemainingMs / 1000)} s.</li>
      </ul>
    </details>}
  </>;
}
