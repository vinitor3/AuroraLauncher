import { socialLabBuild } from "../../lib/firebase";

export function SocialBuildBanner() {
  return <p className="eyebrow">{socialLabBuild
    ? "TESTE LOCAL · CONTAS DESCARTÁVEIS · SEM PUBLICAÇÃO NA COMUNIDADE REAL"
    : "VERSÃO EXPERIMENTAL · SERVIÇO ONLINE"}</p>;
}
