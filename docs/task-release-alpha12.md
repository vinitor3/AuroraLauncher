# Instalador integrado 0.1.1-alpha.12

Estado: **CONCLUÍDO** — integração e instalador gerados. Homologação runtime completa: **NÃO VERIFICADO**.

Base ativa: `6f3a13b` (alpha.11), Aurora Core 1.1.5 e Aurora Skins 0.3.1. A consulta de tarefas e `release:status` encontrou somente uma entrega pendente: `codex/aur-doctor-gemini`, commit `0ed345f`. Ela foi integrada por fast-forward na worktree exclusiva `codex/aur-release-alpha12`, preservando todas as entregas anteriores e sem alterações nos mods.

A versão inclui o Doctor com identificação ampliada e explicação Gemini curta por ação explícita, além de tradução, catálogo/grade/cache/multiplayer de skins e sessões independentes já integrados. Os testes da entrega Doctor comprovaram 158 testes Rust, 104 Worker, 19 transporte/Assistente, 21 grupos DOM e análise nativa de uma instância real; detalhes em [task-doctor-gemini.md](task-doctor-gemini.md).

Versões sincronizadas em manifestos npm, lockfile desktop, Cargo e Tauri. O pacote foi gerado uma única vez pelo fluxo `npm run release:package`, que executou o build oficial, recompilou e conferiu os 18 JARs e produziu SHA-256/manifesto. Nenhum artefato histórico foi substituído.

A revisão independente confirmou o empacotamento dos contratos TypeScript compartilhados pelo Vite e a compatibilidade da CSP. O diretório novo `shared` foi incluído nas listas de produto do status/preflight para que alterações pendentes em worktrees vizinhas também bloqueiem uma release.

## Artefato e evidências

- Instalador: `releases/Aurora Smart Launcher_0.1.1-alpha.12_x64-setup.exe`, Windows x64, 7.888.064 bytes.
- SHA-256: `4c887fef1c95ad3dac83ce310bbb19cc0db3c32ffa8283e3f0f480f56d560adc`, conferido com o arquivo e manifesto.
- Fonte do build: `42abd2d111e9f871b0dc4c5b51bf12eed9291c96`, branch `codex/aur-release-alpha12`, checkout limpo.
- Build concluído em 23/09/2026; artefatos conferidos e entrega finalizada em 28/09/2026 após retomada. O build não foi repetido.
- Preflight inicial/final: **CONCLUÍDO**, 25 worktrees conferidas. Nenhuma entrega de produto pendente na retomada.
- Matriz recompilada: **CONCLUÍDO**, 18/18 JARs Core 1.1.5 e Skins 0.3.1.
- Build React/TypeScript e NSIS: **CONCLUÍDO**; aviso existente de tamanho do bundle. Os 19 testes de transporte/Assistente também passaram após o ajuste de versão.
- `.exe.sha256` e `.exe.manifest.json` acompanham o instalador. O manifesto registra os hashes dos módulos e o commit exato da compilação.
- Log local ignorado pelo Git: `.aurora/release-runs/20260923-095022-06c3fa55/build.log`.

Limites: publicar o Worker não faz parte de gerar o instalador. A consulta Gemini real e a homologação completa em Minecraft permanecem **NÃO VERIFICADO**. O instalador não declara assinatura Authenticode nem atualização automática.
