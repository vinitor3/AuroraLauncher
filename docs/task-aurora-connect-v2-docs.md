# Tarefa — revisão documental Aurora Connect V2

Data: 29/09/2026. Estado da entrega documental: **CONCLUÍDO**. Transporte/integrações novos descritos: **NÃO IMPLEMENTADO**; homologação: **NÃO VERIFICADO**. [Índice dos sete entregáveis](engineering/AURORA_CONNECT_REVIEW.md).

## Base, objetivo e escopo

- Repositório de origem: `C:/Users/usuario/Desktop/AuroraLauncher`.
- Base: `dc2ec5ac36a88f54b1130bac4789f8ecd56641d2`; launcher `0.1.1-alpha.13`, Core `1.1.5`, Skins `0.3.1`.
- Branch: `codex/aur-r4-connect-v2-docs`.
- Worktree: `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect-v2-docs`.
- Objetivo: revisar o roadmap completo, especificar serviços TCP/UDP e compatibilidade de voz, documentar política corrigida de relay/zero-spend e produzir prompt autossuficiente para o próximo incremento.
- Escopo autorizado nesta execução: documentação e leitura/pesquisa. Não houve implementação, instalação de mods, teste de transporte/Minecraft, alteração de configuração do usuário, contratação, billing, push, release ou deploy.
- Dependências: instruções do usuário V2, `AGENTS.md`, roadmap canônico e documentos/fontes realmente disponíveis. Não foi encontrado `MASTER_ROADMAP(1).md` na busca delimitada; leitura integral feita no caminho canônico. Outras worktrees e o checkout de origem foram preservados.

O recurso de worktree gerenciada do chat falhou com “Not a git repository” no contexto TESTE-main. A worktree Git foi criada explicitamente no repositório Aurora usando o HEAD já conferido, fora de outros checkouts. Nenhum trabalho existente foi movido ou arquivado.

## Arquivos produzidos e atualizados — lista completa

Todos os caminhos abaixo são relativos à raiz da worktree informada. São **20 arquivos Markdown**, oito novos e doze atualizações pontuais; não existe segunda cópia de roadmap criada por esta tarefa.

| Arquivo | Ação | Finalidade |
| --- | --- | --- |
| `docs/engineering/AURORA_CONNECT_REVIEW.md` | novo | Diagnóstico, autoridade/limites e ordem dos sete entregáveis |
| `docs/engineering/MASTER_ROADMAP.md` | atualizado | Roadmap completo preservado e R4 detalhada |
| `docs/engineering/AURORA_CONNECT_SPEC.md` | novo | Arquitetura, TCP/UDP, serviços, voz, UX, segurança e testes |
| `docs/engineering/ADR/PROPOSED-aurora-connect-transport.md` | novo | ADR proposta sem número inventado, alternativas/pendências |
| `docs/engineering/AURORA_CONNECT_BACKLOG.md` | novo | Inventário estático, sete tarefas e matrizes |
| `docs/engineering/AURORA_CONNECT_DEPENDENCIES.md` | novo | Operadores, fontes, termos, quotas, custo/falhas e autorização por uso |
| `docs/engineering/AURORA_CONNECT_TRACEABILITY.md` | novo | Requisito → documento → tarefa → evidência e changelog |
| `docs/engineering/CODEX_PROMPTS/AUR-R4-connect-v2.md` | novo | Prompt executável posterior, autossuficiente |
| `docs/engineering/ADR/ADR-010-multiplayer.md` | atualizado | Preserva original e explicita precedência da política/estratégia V2 |
| `docs/engineering/ADR/README.md` | atualizado | Índice da proposta, sem declarar decisão aceita |
| `docs/engineering/ARCHITECTURE_REVIEW.md` | atualizado | Resumo multiplayer alinhado aos contratos TCP/UDP |
| `docs/engineering/CODEX_PARALLEL_PLAN.md` | atualizado | Remove previsão residual de HUD e preserva ownership/gates |
| `docs/engineering/CODEX_PROMPTS/README.md` | atualizado | Link do prompt R4, sem mudar prioridade Wave 0 |
| `docs/engineering/CODEX_TASKS/README.md` | atualizado | Índice dos IDs R4 em documento único de backlog |
| `docs/engineering/COMPATIBILITY_MATRIX.md` | atualizado | Estado documental e referência às células de voz não homologadas |
| `docs/engineering/DECISIONS_REQUIRED.md` | atualizado | D-003 revisada; separa requisito de escolha técnica pendente |
| `docs/engineering/README.md` | atualizado | Entrada da entrega no índice de engenharia |
| `docs/engineering/RESEARCH_REPORT.md` | atualizado | Nota de autoridade atual; pesquisa antiga permanece histórica |
| `docs/engineering/RISK_REGISTER.md` | atualizado | R-013: risco de custo/termos, sem proibir relay gratuito |
| `docs/task-aurora-connect-v2-docs.md` | novo | Este registro de escopo, entrega, validação e rollback |

## Critério de aceite e validação documental

Aceite: sete entregáveis completos, em português e indexados na ordem pedida; todas as fases/IDs/gates preservados; TCP/UDP/voz explícitos; relay gratuito permitido e custo automático vedado; nenhum recurso promovido a homologado; fontes/versões/data e caminhos novos identificados; prompt posterior exige incremento efetivo e testes reais.

Verificações realizadas nesta entrega:

1. Leitura integral do roadmap original no SHA base e revisão do diff: organização, escala, fases, waves e IDs existentes preservados; R4 ampliada e instrução residual de HUD corrigida conforme ADR-018. Busca das três proibições antigas de relay indicadas pelo usuário: nenhuma ocorrência como regra no pacote final.
2. Matriz de nove targets no backlog comparada com `scripts/active-mod-matrix.json`, sem divergências. Arquivo da matriz não alterado; SHA-256 `96C4EC789902F8AA87931ECD72ACA531FEC53D4AC9C6055FCB9837EF77E6918A` nesta base.
3. Conferência de links Markdown locais nos arquivos da entrega, presença de fases/IDs e escopo exclusivo de documentação: **20 arquivos Markdown, 147 links locais válidos, oito IDs preexistentes e nove targets; zero falhas**. O verificador documental foi executado em PowerShell, sem acrescentar código ao repositório; resultado preservado no histórico desta tarefa.
4. `git diff --check` e conferência de todo o diff, incluindo documentos novos após staging; sem erros de whitespace. Manifests, lockfiles, código, JARs, matriz e contratos de features não foram alterados.
5. Revisão independente por dois agentes: sem bloqueador arquitetural/preservação encontrado. Ajustes aplicados: `serviceId` único no contrato, linguagem que limita a conclusão do inventário, grafia de retenção e campos completos de dependências dos mods/catálogos. Seleção Iroh e APIs foram confrontadas com fontes oficiais, sem teste runtime.

Não executados, por escopo documental: build desktop/Rust/Worker/Functions/Java, testes unitários/de integração/emulador, Minecraft, NAT/relay, instalação de mods, ensaio de áudio e benchmarks. Esses testes não são necessários para validar Markdown e não receberam status aprovado. Não houve auditoria completa do repositório ou de contas/infraestrutura.

Consulta externa foi pesquisa de documentação oficial. O Dokka Plasmo 2.1.17 foi lido na pesquisa de especificação, mas uma reconsulta independente apresentou erro de acesso; o contrato completo de mutadores continua pendente de fonte/artefato versionado. Essa falha não foi ocultada nem usada como prova de API inexistente. API de voz pesquisada não fixa JAR/loader/hash de homologação.

## Resultado, riscos e decisões abertas

O candidato principal de transporte é Iroh 1.1.0 para investigação, condicionado a toolchain/segurança/API/build e testes. Nenhum relay de produção está aprovado. As políticas permitem relay gratuito após verificação do uso; indisponibilidade mantém direto ou falha clara, sem contratação. Datagramas reais de voz podem exceder o limite inicial proposto; endereçamento LAN/misto e vínculo peer↔jogador exigem prova. Os gates anteriores do roadmap permanecem intactos.

Riscos restantes: documentação comercial mudar, contas não verificadas, fontes `latest` divergirem de artefatos, contratos de sinalização ainda ausentes, busca focal não localizar trabalho de outra branch, testes runtime pendentes. Mitigação: o prompt começa por inventário atual e avança ao próximo bloqueio real, preservando trabalho comprovado.

## Rollback e integração

Rollback é reverter o commit documental em mudança revisável, sem reset destrutivo, sem apagar alterações de outros agentes e sem tocar em mundos/configurações. Nenhum estado externo precisa ser desfeito. O checkout de origem não foi atualizado pela tarefa; integrar somente após revisar este conjunto, conservando mudanças concorrentes. O SHA do commit documental é apresentado na entrega e consultável no histórico da branch; este arquivo não se autorreferencia com hash calculado antes do commit.

Próxima tarefa usa [o prompt Codex](engineering/CODEX_PROMPTS/AUR-R4-connect-v2.md): revalidar inventário e gates, refinar a decisão e implementar o menor incremento autenticado verificável. Esta conclusão refere-se somente à entrega documental.
