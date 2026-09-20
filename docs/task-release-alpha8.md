# Release 0.1.1-alpha.8

Preparação: 2026-09-17; integração final: 2026-09-19.
Branch: `codex/aur-release-alpha8`, base `07f6edb`.
Pedido: somente novo instalador; sem novas funcionalidades.

Inclui a alpha.7 e o reforço visual de carregamento validado em
`task-startup-motion.md`: anéis, logo, barra e pontos animados; movimento
reduzido conserva pulsação suave, sem deslocamento espacial.

Empacotamento: **CONCLUÍDO**, exclusivamente com `npm run release:package`,
preservando gates, matriz 27/27, SHA-256, manifesto e releases anteriores.
Nenhum deploy, push ou execução automática do instalador realizado.

Integra também a entrega concluída `269d282fc76ab5581a6904a854b1335c70626097`,
documentada em `task-skins-runtime-repair.md`: Core **1.1.4**, Skins **0.2.4**
e Companion **0.2.3**. Core 1.1.3 permanece candidato histórico, não selecionado.
A entrega altera Rust e Java; os gates do conjunto foram repetidos, com as
matrizes Java sequenciais para evitar disputa pelo cache compartilhado Loom.
Não há mudança de Worker nesta integração.
Gameplay permanece **IMPLEMENTADO MAS NÃO HOMOLOGADO**.

## Evidências finais

Fonte do build: `6cacd5863ee7bb243a504943a216275e8d26337b`, checkout limpo.
Os preflights inicial, intermediário e final aprovaram as 16 worktrees.

- Node desktop: 56 testes aprovados.
- Rust: fmt e clippy aprovados; 90 testes aprovados, 3 ignorados, sem falhas.
- Matrizes Core, Skins e Companion: 9/9 cada, recompiladas sequencialmente;
  27/27 artefatos publicados correspondem ao código atual.
- TypeScript/Vite, Rust release e NSIS aprovados. O aviso de bundle frontend
  acima de 500 kB não bloqueou o build.
- Revisão independente confirmou versões, hashes e assinaturas Ed25519 do
  Core contra a chave pública fixada. Isso não é assinatura do instalador.
- Executável nativo Windows x64: PE Machine `0x8664`, Subsystem `2` (GUI).

Artefato: `releases/Aurora Smart Launcher_0.1.1-alpha.8_x64-setup.exe`,
6.780.308 bytes, produto e versão PE conferidos.

SHA-256: `0cd13e4e0eda106ab292bb7010d228e167f06da527883161c752d9fc25ba1652`.
O `.sha256` e o `.manifest.json` acompanham o instalador e registram fonte,
tamanho e os 27 hashes dos módulos. Nenhuma release anterior foi sobrescrita.

Authenticode do instalador: `NotSigned`; Windows pode avisar sobre editor
desconhecido. Instalação e homologação integral em Minecraft **PRECISA DE
TESTE**. Evidências parciais de runtime continuam no documento da entrega,
sem promovê-las a homologação de todas as versões.

Logs locais ignorados: `.aurora/logs/alpha8-final-*` e
`.aurora/release-runs/20260919-233938-ec8032e8/build.log`.
