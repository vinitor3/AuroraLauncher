# ADR-016 — Fontes de skins centralizadas no Worker

Data: 2026-09-16. Estado: aceita para implementação.

## Contexto

Aurora suporta mods Java 8/17/21, Fabric e Forge. Incorporar chave privada ou um novo cliente com dependências em cada JAR aumenta exposição de segredo e risco de conflitos (como SLF4J no Cave Horror). mc-api resolve perfis e nomes, mas não oferece categorias de skins.

## Decisão

Concentrar HTTP, DTOs, cache, limitação e fallback em TypeScript/fetch no Worker existente. mc-api é fonte principal para nome/UUID e histórico observado; MCStat é catálogo por tema/tag e fallback de perfil. Só o Worker recebe MC_API_KEY/mcstat_API_KEY por secret. Sem MC_API_KEY, usar mc-api público.

Expor rotas Aurora públicas limitadas, JSON para desktop e TSV V1 para compatibilidade Java 8. PNGs exigem allowlist de origem, dimensões/estrutura/CRC e SHA-256. Mods mantêm Aurora Core como ponto de entrada/configuração; a prévia do resultado tem textura própria e não modifica a aparência ativa.

## Consequências

Os adaptadores compartilham uma implementação compatível com Java 8. A descoberta online depende do Worker. A biblioteca local funciona sem rede. O cache em memória e os limites de requisições operam por isolate. Uma cota global exigiria coordenação entre regiões.

Versionar novos JARs como 0.2.2; preservar 0.2.1 e instalador alpha.5. Mudanças de outras worktrees não são descartadas nem incluídas silenciosamente em um instalador.
