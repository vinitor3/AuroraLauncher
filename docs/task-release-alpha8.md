# Release 0.1.1-alpha.8

Preparação: 2026-09-17; integração final: 2026-09-19.
Branch: `codex/aur-release-alpha8`, base `07f6edb`.
Pedido: somente novo instalador; sem novas funcionalidades.

Inclui a alpha.7 e o reforço visual de carregamento validado em
`task-startup-motion.md`: anéis, logo, barra e pontos animados; movimento
reduzido conserva pulsação suave, sem deslocamento espacial.

Preparação: **PARCIAL**. Empacotar exclusivamente com `npm run release:package`,
preservando gates, matriz 27/27, SHA-256, manifesto e releases anteriores.
Nenhum deploy, push ou execução do instalador solicitado.

Integra também a entrega concluída `269d282fc76ab5581a6904a854b1335c70626097`,
documentada em `task-skins-runtime-repair.md`: Core **1.1.4**, Skins **0.2.4**
e Companion **0.2.3**. Core 1.1.3 permanece candidato histórico, não selecionado.
A entrega altera Rust e Java; os gates do conjunto serão repetidos, com as
matrizes Java sequenciais para evitar disputa pelo cache compartilhado Loom.
Não há mudança de Worker nesta integração.
Gameplay permanece **IMPLEMENTADO MAS NÃO HOMOLOGADO**.
