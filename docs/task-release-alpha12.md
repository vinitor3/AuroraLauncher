# Instalador integrado 0.1.1-alpha.12

Estado: **PARCIAL** — integração concluída, empacotamento em preparação.

Base ativa: `6f3a13b` (alpha.11), Aurora Core 1.1.5 e Aurora Skins 0.3.1. A consulta de tarefas e `release:status` encontrou somente uma entrega pendente: `codex/aur-doctor-gemini`, commit `0ed345f`. Ela foi integrada por fast-forward na worktree exclusiva `codex/aur-release-alpha12`, preservando todas as entregas anteriores e sem alterações nos mods.

A versão inclui o Doctor com identificação ampliada e explicação Gemini curta por ação explícita, além de tradução, catálogo/grade/cache/multiplayer de skins e sessões independentes já integrados. Os testes da entrega Doctor comprovaram 158 testes Rust, 104 Worker, 19 transporte/Assistente, 21 grupos DOM e análise nativa de uma instância real; detalhes em [task-doctor-gemini.md](task-doctor-gemini.md).

Versões sincronizadas em manifestos npm, lockfile desktop, Cargo e Tauri. O pacote será gerado somente pelo fluxo `npm run release:package`, que executa o build oficial, recompila e confere os 18 JARs e produz SHA-256/manifesto. Nenhum artefato histórico será substituído.

A revisão independente confirmou o empacotamento dos contratos TypeScript compartilhados pelo Vite e a compatibilidade da CSP. O diretório novo `shared` foi incluído nas listas de produto do status/preflight para que alterações pendentes em worktrees vizinhas também bloqueiem uma release.

Limites: publicar o Worker não faz parte de gerar o instalador. A consulta Gemini real e a homologação completa em Minecraft permanecem **NÃO VERIFICADO**. O instalador não declara assinatura Authenticode nem atualização automática.
