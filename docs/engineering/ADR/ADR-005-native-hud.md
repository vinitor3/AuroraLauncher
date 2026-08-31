# ADR-005 — Screen interativa + HUD de legendas nativos

## Contexto

Swing externa disputa foco e não é UI in-game. O painel precisa texto/input; legendas precisam aparecer sem capturar mouse.

## Opções avaliadas

1. manter Swing;
2. somente HUD custom desenhada;
3. `Screen` cliente para interação + camada HUD passiva para legenda/status.

## Vantagens e desvantagens

`Screen` oferece foco, widgets e acessibilidade; uma HUD passiva não bloqueia gameplay. Uma única camada HUD interativa exigiria reimplementar input/foco.

## Riscos

Tela pausar mundo, prender cursor ou consumir keybind; APIs mudam por versão.

## Decisão recomendada

**SUBSTITUIR** Swing pela opção 3. MVP Fabric 1.20.1, depois Forge 1.20.1. A tela não pausa o mundo, restaura foco ao fechar e screenshot exige confirmação. Swing permanece somente como fallback de desenvolvimento até os gates runtime.

## Consequências

Dois renderers pequenos sobre o mesmo view-model. Evidência manual/automatizada de foco é obrigatória.

## Reversibilidade

Alta enquanto o IPC/view-model permanecer independente.

