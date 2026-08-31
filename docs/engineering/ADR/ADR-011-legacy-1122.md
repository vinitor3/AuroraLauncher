# ADR-011 — Minecraft 1.12.2 como tier legado

## Contexto

Forge 1.12.2 exige Java 8, LWJGL2, ASM/classloader antigos e adaptação própria. Já há runtime comprovado, mas HUD/P2P modernos não portam diretamente.

## Opções avaliadas

1. paridade simultânea em toda feature;
2. remover suporte;
3. preservar launch/Companion essencial e portar recursos depois do moderno.

## Vantagens e desvantagens

Paridade agrada packs legados, mas multiplica prazo/risco. Remoção quebra valor existente. Tier legado mantém suporte honesto sem bloquear arquitetura atual.

## Riscos

Dependências sem manutenção e correções de segurança indisponíveis.

## Decisão recomendada

**MANTER como legado**: build isolado, Java 8, runtime smoke e correções críticas. HUD/updater/multiplayer entram somente após 1.20.1 e não bloqueiam gates modernos.

## Consequências

Matriz e UI mostram nível de suporte por capacidade, não um “suporta” binário.

## Reversibilidade

Média; remoção futura exige aviso/migração, mas módulo permanece desacoplado.

