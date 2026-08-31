# ADR-012 — Creator MVP determinístico antes de suites complexas

## Contexto

KubeJS é LGPL-3.0 e permite acesso a classes Java; CraftTweaker é MIT; FTB Quests declara All Rights Reserved. Gerar scripts arbitrários com IA amplia execução de código.

## Opções avaliadas

1. editor FTB Quests/SNBT;
2. gerador KubeJS genérico;
3. gerador CraftTweaker;
4. editor de recipes/tags em modelo próprio com export datapack e KubeJS restrito.

## Vantagens e desvantagens

FTB tem UX conhecida, mas licença/formato e escopo são riscos. KubeJS é popular/flexível, inclusive flexível demais. Modelo próprio é pequeno, validável e não obriga runtime de script.

## Riscos

IA gerar `Java.loadClass`, comandos ou conteúdo incompatível; copiar formato/ativos ARR.

## Decisão recomendada

**SUBSTITUIR o primeiro MVP** pela opção 4: recipes/tags determinísticos, preview/diff e export datapack; adaptador KubeJS apenas para templates allowlisted, sem Java/reflection. CraftTweaker vem depois. FTB Quests espera esclarecimento/licença e parser SNBT isolado.

## Consequências

Entrega menor, testável e reaproveitável pelo Server Pack Generator.

## Reversibilidade

Alta; o modelo intermediário pode ganhar exportadores.

