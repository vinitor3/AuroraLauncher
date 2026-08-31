# ADR-002 — Deprecar Firebase Functions

## Contexto

Desktop usa Cloudflare Worker; `functions/` mantém implementação Gemini antiga, dependências e superfície de deploy duplicada. Auditoria encontrou advisories moderados transitivos.

## Opções avaliadas

1. manter ambos ativos;
2. consolidar no Worker e remover Functions após auditoria;
3. voltar tudo a Functions.

## Vantagens e desvantagens

Duplicação oferece fallback aparente, mas dobra manutenção e pode divergir em auth/política. Worker já é a rota configurada e possui health/storage/CurseForge.

## Riscos

Remover endpoint ainda usado por build antigo; esquecer secret/deploy legado.

## Decisão recomendada

**REMOVER**, em duas etapas: marcar LEGADO e buscar tráfego/referências por um ciclo; depois apagar código/config/deploy e revogar secrets. Não adicionar features.

## Consequências

Uma API ativa, menos advisories. Releases antigas que dependam de Functions precisam aviso/fallback explícito.

## Reversibilidade

Alta via tag histórica enquanto configuração e secrets não forem destruídos; recriar somente por ADR.

