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

Preparação: **PARCIAL**. Executar suíte Node, fmt/clippy/test Rust e
`npm run release:package` (preflight, matriz dos 27 mods e build Tauri).
Logs em `.aurora`; atualizar este documento após empacotamento.

O manifesto final registra commit, versão, tamanho, SHA-256 e hashes dos JARs.
Nenhum gate será desabilitado nem artefato anterior sobrescrito.

Gameplay: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. A próxima build instalada e o
comportamento no Minecraft ainda **PRECISA DE TESTE**.
Assinatura digital: **NÃO VERIFICADO** nesta preparação.
