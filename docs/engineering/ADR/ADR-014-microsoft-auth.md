# ADR-014 — Entitlement oficial antes de distribuição pública

## Contexto

Aurora usa nick/senha próprios e identidade offline determinística. Isso não comprova posse de Minecraft e pode ser percebido como launcher “cracked”. EULA/Usage Guidelines proíbem redistribuir o jogo e exigem evitar aparência oficial.

## Opções avaliadas

1. manter offline irrestrito;
2. Microsoft/Xbox/Minecraft auth com entitlement para jogo adquirido;
3. offline apenas para desenvolvimento/servidores explicitamente offline.

## Vantagens e desvantagens

Offline é simples e inclui usuários sem conta, mas cria alto risco legal/ecossistêmico e incompatibilidade com soluções como Essential. Auth oficial melhora legitimidade e online-mode, porém exige app registration/fluxo complexo.

## Riscos

Client ID não autorizado, armazenamento inseguro de refresh token, mudança de API/termos e promessa falsa de acesso.

## Decisão

**DECISÃO REGISTRADA: não, por enquanto.** O Aurora permanece como protótipo de desenvolvimento privado e não deve publicar um fluxo de distribuição pública baseado em token fictício ou sem entitlement Microsoft/Minecraft. A autenticação oficial continua sendo requisito para uma futura distribuição pública.

## Consequências

Criar estudo de app registration, device code/PKCE, token vault do SO, ownership/profile e revogação. Firebase continua identidade Aurora, não entitlement Minecraft.

## Reversibilidade

Média; contratos `AuroraIdentity` e `MinecraftEntitlement` devem permanecer separados.
