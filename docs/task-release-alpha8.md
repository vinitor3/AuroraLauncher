# Release 0.1.1-alpha.8

Data: 2026-09-17. Branch: `codex/aur-release-alpha8`, base `07f6edb`.
Pedido: somente novo instalador; sem novas funcionalidades.

Inclui a alpha.7 e o reforço visual de carregamento validado em
`task-startup-motion.md`: anéis, logo, barra e pontos animados; movimento
reduzido conserva pulsação suave, sem deslocamento espacial.

Preparação: **PARCIAL**. Empacotar exclusivamente com `npm run release:package`,
preservando gates, matriz 27/27, SHA-256, manifesto e releases anteriores.
Nenhum deploy, push ou execução do instalador solicitado.

Não houve mudança Rust/Java/Worker além da versão do pacote; a suíte Rust
88/88 de alpha.7 continua evidência da mesma implementação nativa.
Gameplay permanece **IMPLEMENTADO MAS NÃO HOMOLOGADO**.
