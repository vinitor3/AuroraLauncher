# AUR-RELEASE — 0.1.1-alpha.7

Data: 2026-09-17. Branch: `codex/aur-release-alpha7`, base `20f9b46`.

## Escopo

Instalador único solicitado pelo usuário, preservando alpha.6 e suas releases
anteriores. Todas as 12 worktrees anteriores estavam limpas e integradas antes
de preparar esta versão.

Inclui as entregas de alpha.6 e, adicionalmente:

- `46412f7`: fila automática de downloads externos autorizados e verificação
  por hash, sem contornar restrições dos autores.
- `7df9b0e`: animação durante restauração de sessão e processos Windows sem
  console visível, preservando logs e o login de quem não possui sessão.
- `20f9b46`: aba Equipada funcional, persistência do PNG e modelo local,
  recuperação na reabertura e tratamento de imagens indisponíveis.

Não houve mudança de Worker nem fontes Java desde alpha.6; nenhum deploy
externo faz parte desta release. Contas, instâncias e skins pessoais preservadas.

## Validação e publicação

Empacotamento: **CONCLUÍDO**. Fonte do build: `3799143` (checkout limpo).

- Suíte Node desktop: 50 testes aprovados.
- Rust: fmt e clippy aprovados; 88 testes aprovados e 3 testes online opcionais
  ignorados, sem falha. Execução serial para evitar instabilidade conhecida IPC.
- Build oficial via `npm run release:package` e `npm run desktop:build`:
  Core 1.1.2, Skins 0.2.3 e Companion 0.2.3, 9/9 arquivos em cada matriz.
- Build TypeScript/Vite e Tauri/NSIS aprovados; preflights inicial e final
  aprovaram as 13 worktrees. Nenhum gate desabilitado.
- Logs: `.aurora/logs/alpha7-*` e
  `.aurora/release-runs/20260917-103653-491d3844/build.log`.

Artefato: `releases/Aurora Smart Launcher_0.1.1-alpha.7_x64-setup.exe`,
6.764.928 bytes. Produto e versão dos metadados PE conferidos.

SHA-256: `7c2fea709c80dc94c19e028e99a7460da42178d65be1302f887ac83eb5169eb1`.
O `.sha256` e o `.manifest.json` acompanham o instalador; o manifesto registra
fonte, tamanho e os 27 hashes dos módulos. Releases anteriores preservadas.

Gameplay: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. A próxima build instalada e o
comportamento no Minecraft ainda **PRECISA DE TESTE**.
Authenticode do instalador: `NotSigned` (conferido após o build). O Windows
pode mostrar aviso de editor desconhecido. O instalador não foi executado
automaticamente; nenhum push ou deploy foi realizado.
