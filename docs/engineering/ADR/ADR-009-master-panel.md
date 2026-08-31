# ADR-009 — Master Panel estático e adiado

## Contexto

Não existem manifesto, publicação, roles seguras nem auditoria. Next.js adicionaria SSR/runtime antes de haver backend estável.

## Opções avaliadas

1. Next.js full-stack agora;
2. SPA React estática + Worker API;
3. nenhuma UI até operar tudo manualmente.

## Vantagens e desvantagens

Next.js acelera produto web, mas aumenta deploy e custo cognitivo. SPA reutiliza stack e hospeda gratuitamente; não resolve contratos. Adiar UI reduz distração.

## Riscos

Construir painel sobre `role` autodeclarada, vazar service role ou criar autorização apenas no cliente.

## Decisão recomendada

**ADIAR** UI até manifesto v3, promoção server-side e audit log. No MVP, escolher opção 2: assets estáticos, Firebase Auth e Worker validando cada ação/role. Reavaliar Next.js somente se SSR/server components trouxerem requisito comprovado.

## Consequências

Master não bloqueia launcher. Todas as APIs são utilizáveis/testáveis sem painel.

## Reversibilidade

Alta; frontend pode migrar mantendo API.

