# ADR-021 — Aurora Doctor local e somente leitura

- Data: 2026-09-22
- Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**
- Escopo: diagnóstico de instâncias e de tentativas de inicialização no launcher desktop.

## Contexto

O launcher já possui perfil de instância, seleção de Java, preparação de lançamento, acompanhamento de processos e logs locais. Faltava transformar esses dados em explicações verificáveis sem atribuir a uma tentativa um crash antigo, expor informações privadas ou introduzir mutações automáticas.

## Decisão

1. Usar regras determinísticas no engine Rust e executá-las em trabalho bloqueante fora da UI. O diagnóstico básico é offline e não usa Gemini, Worker ou outro serviço externo.
2. Registrar a última tentativa por instância no diretório privado de dados do launcher, fora da instância. O registro inclui um identificador opaco, contexto de lançamento, resultado observado e identificação limitada dos logs anteriores. Ao consultar a execução, o launcher pode registrar um encerramento recém-observado nesse journal; a análise não altera arquivos da instância.
3. Tratar sucesso, cancelamento, preparação/execução em andamento, falha e resultado desconhecido como estados distintos. Se o launcher perde o observador do processo, não inferir crash. Uma solicitação referente a uma execução substituída deve ser recusada.
4. Correlacionar logs por metadados e prefixo limitado coletados antes da preparação. Excluir conteúdo histórico e considerar apenas o sufixo novo quando o arquivo é anexado. Limites de leitura e associação incompleta devem aparecer no relatório.
5. Construir evidências compartilháveis por lista de marcadores e textos controlados. Não copiar linhas livres, nomes de mods/jogadores, caminhos pessoais ou mensagens arbitrárias de erro. IDs de execução e campos opcionais passam por validação própria. O relatório não é um arquivo de log integral.
6. Reutilizar os requisitos base de Java, o perfil de lançamento e os controles existentes de confinamento de arquivos. Esta entrega não cria outro inventário/resolvedor de mods nem executa o Java durante a análise.
7. Manter toda recuperação automática fora deste incremento. Nenhum mod é desativado, arquivo apagado, configuração alterada ou download iniciado pelo Doctor.

## Consequências e limites

O resultado é reproduzível e pode explicar evidências explícitas, mas não identifica toda causa possível. As regras distinguem o erro observado de uma hipótese causal, em especial para rede/serviços opcionais. Uma stack trace que cita um mod não prova responsabilidade.

A privacidade tem custo de detalhe: o relatório omite trechos que poderiam ajudar uma investigação manual. O Java selecionado é examinado por existência; sua versão é aproveitada apenas quando já foi observada no contexto compatível da tentativa. Requisitos adicionais de mods dependem de erros explícitos, sem um segundo resolvedor.

Um próximo incremento de recuperação deve definir ações estreitas, confirmação, exclusão mútua, backup/rollback e auditoria antes de qualquer mutação. CAS, modo seguro, backup geral e restauração não foram implementados aqui.

Comportamento: [Aurora Doctor](../../aurora-doctor.md). Evidências: [registro da entrega](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/task-aurora-doctor.md).
