# Aurora Launcher 0.1.1-alpha.10

Entrega: correção da tradução e integração do Aurora Doctor concluído em outra
tarefa. A fonte de tradução foi corrigida na branch `codex/aur-translation-repair`;
a entrega Doctor `8551c54` foi integrada preservando as mudanças da alpha.9.

## Comportamento

- Tradução de páginas com idiomas misturados por trecho, lotes menores, progresso
  visível, cancelamento, retomada e restauração do original. Associação por IDs,
  fallback para resposta incompleta e espaços portugueses em fronteiras CJK.
- Recuperação do endpoint ausente em configuração oficial antiga; nenhuma resposta
  de projeto antigo pode liberar carregamento ou substituir o projeto atual.
- Zombie Invade 100 Days foi usado como caso público real de renderização e
  divisão da página chinesa/inglesa. Testes de tradução usam respostas simuladas.
- Doctor: aba Diagnóstico, registro de execução e relatório local sanitizado.
  Mantém os limites da entrega documentada em `task-aurora-doctor.md`; não inclui
  correção automática de instâncias nem homologação de falha real no Minecraft.
- A revisão de integração corrigiu uma falha no Doctor: journal corrompido ou
  inacessível não bloqueia mais o lançamento. O estado permanece em memória;
  o arquivo original fica intacto e as proteções concorrentes são preservadas.
- Core 1.1.5 e Skins 0.2.5 preservados. O Assistente desktop permanece e Companion
  continua removido.

## Verificações

- 140 testes frontend e 90 testes Worker aprovados.
- 135 testes Rust aprovados (89 biblioteca, 31 binário, 15 QA), cinco ignorados
  por padrão. Os testes ignorados não foram contabilizados como aprovados.
- 13 verificações DOM do Doctor aprovadas; build desktop aprovado.
- Formatação e Clippy aprovados; nove testes do journal aprovados, incluindo
  corrupção, falha de escrita, concorrência e junction sem escrita externa.
  A suíte Rust completa e a matriz dos 18 JARs são conferidas na integração e
  no empacotamento oficial. O resultado final fica junto ao artefato.
- Worker de tradução publicado: `05e6b678-6847-4c0b-a4da-7a03a6b3dc67`, endpoint
  `https://aurora-api.aurora-edge-proxy.workers.dev`.
- Tradução autenticada da nova versão no Gemini: **NÃO VERIFICADO**. A abertura
  do launcher com depuração foi bloqueada pela revisão automática. Não houve
  extração de credenciais nem captura de tela.

O instalador é produzido por `npm run release:package`, que chama o build oficial
`npm run desktop:build`, após commit e preflight. A alpha.9 e demais artefatos
anteriores permanecem imutáveis. Hash, tamanho e commit de origem devem ser lidos
no manifesto adjacente ao instalador alpha.10. Assinatura Authenticode e
homologação completa em Minecraft: **NÃO VERIFICADO**.

## Artefato final

Empacotamento: **CONCLUÍDO**, preflights inicial/final aprovados, 21 worktrees
conferidas e matriz de módulos **18/18**.

- Arquivo: `releases/Aurora Smart Launcher_0.1.1-alpha.10_x64-setup.exe`.
- Fonte: `f5a3c7d38c7fd81a926144d3c2e7f80634a0c623`.
- Tamanho: **6.897.760 bytes**.
- SHA-256: `4f9ed16c517bc08794f50220447e6a7e137ca9404c9d18271e754020de9235cf`.
- Hash, tamanho e versão PE conferidos independentemente contra o manifesto.
- Comando: `npm run release:package`; log local ignorado em
  `.aurora/release-runs/20260922-120005-57a16478/build.log`.

Não foi publicado release no GitHub nem enviado commit remoto. O instalador
e a tag são locais; o deploy do Worker de tradução foi concluído separadamente.
