# ADR-025 — Gemini compartilhado e ações locais confirmadas

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. Data: 28/09/2026. Complementa ADR-008 e amplia o alcance manual da ADR-024 sem alterar o diagnóstico determinístico do Doctor.

## Contexto

O cliente possui fluxos Gemini separados e a produção inspecionada antecedia a rota do Doctor. Erros genéricos impediam distinguir implantação antiga, chave, quota e indisponibilidade. O jogador solicita ações sobre a instância e histórico que permita observar se uma tentativa ajudou. Resposta de modelo, log e metadado de mod não constituem autorização de escrita.

## Decisão

Centralizar seleção de modelo, prazo, leitura limitada, fallback restrito e erros públicos no Worker autenticado. Usar `gemini-flash-latest` e, quando aplicável, `gemini-3.8-flash`; nunca repetir por quota/autorização nem ocultar falha com execução automática.

Expor somente `remove_mod` e `set_setting` com IDs temporários emitidos por um broker Rust. O broker reconstrói uma alteração permitida, vincula plano à instância/janela, mostra antes/depois e exige confirmação separada. Revalidar estado e hashes sob bloqueio de mutação com jogo encerrado. Não implementar shell, caminhos livres ou interpretação executável de texto de IA.

Preservar bytes originais e journals autenticados para restauração também confirmada. Confinamento e integridade valem na aplicação e na restauração. As duas superfícies de IA usam os mesmos comandos e a mesma revisão.

Registrar histórico local limitado de diagnóstico, orientação e ação. O texto aceito da IA fica disponível somente para exibição local; o contexto do modelo é reconstruído com categorias e descrições canônicas das tentativas. Uma conclusão `unresolved` depende de categoria conhecida repetida numa execução posterior à alteração. Encerramento normal não prova correção e cancelamento não é crash.

## Consequências

O alcance inicial é reduzido a mods identificados e opções JSON conhecidas. Alterar arquivos, janela, execução ou contexto pode invalidar uma proposta. Backups locais permanecem até manutenção explícita, sem coleta ou upload. O mecanismo não é CAS nem backup integral. Uma resposta correta em mocks ou uma única chamada real não homologa disponibilidade online, qualidade causal ou operação nativa completa.

Evidências e limitações correntes: [task-ai-tools](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/task-ai-tools.md). Contratos e operação: [Aurora AI Tools](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/aurora-ai-tools.md). A matriz de Core/Skins e as versões publicadas permanecem inalteradas.
