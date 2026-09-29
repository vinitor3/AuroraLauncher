# Aurora Connect V2 — diagnóstico e índice da entrega

Data: 29/09/2026. Estado da revisão documental: **CONCLUÍDO** após a conferência registrada no [task file](../task-aurora-connect-v2-docs.md). Novo escopo Connect desta revisão: **NÃO IMPLEMENTADO**; nenhum transporte foi encontrado nas fronteiras examinadas pelo inventário focal, sem prova de inexistência global. Nenhuma homologação foi realizada. “Aurora Connect” é nome de trabalho, não anúncio de funcionalidade pronta.

## Entregáveis na ordem solicitada

1. Este diagnóstico: autoridade, conflitos corrigidos e limites.
2. [Master Roadmap completo revisado](MASTER_ROADMAP.md), no caminho canônico do repositório, preservando R0–R5, waves, prioridades, IDs e gates.
3. [Especificação Aurora Connect](AURORA_CONNECT_SPEC.md): fluxos, serviços, TCP/UDP, mods, segurança, UX e testes.
4. [Proposta de ADR](ADR/PROPOSED-aurora-connect-transport.md): escolha candidata, alternativas e decisões pendentes, sem inventar número disponível.
5. [Backlog e matriz de compatibilidade](AURORA_CONNECT_BACKLOG.md): incrementos, nove pares preservados e homologação por mod/versão/loader/modo/caminho.
6. [Dependências e zero-spend](AURORA_CONNECT_DEPENDENCIES.md) e [rastreabilidade/registro de alterações](AURORA_CONNECT_TRACEABILITY.md).
7. [Prompt completo e autossuficiente para o Codex](CODEX_PROMPTS/AUR-R4-connect-v2.md), para a execução posterior, com inventário, refinamento e primeiro incremento seguro.

## Base e autoridade

Repositório ativo fornecido: `C:/Users/usuario/Desktop/AuroraLauncher`. Base lida: `dc2ec5ac36a88f54b1130bac4789f8ecd56641d2`, branch então ativa `codex/aur-1165-multiplayer`, sem mudanças locais no primeiro `git status --short`. A versão observada nos manifests é launcher `0.1.1-alpha.13`; `scripts/active-mod-matrix.json` seleciona Core `1.1.5` e Skins `0.3.1` em nove pares. Esses números descrevem esta base, não fixam futuras entregas.

`MASTER_ROADMAP(1).md` não foi localizado na busca por nome no workspace, repositório ativo, Desktop e Downloads. Não houve anexo com seu conteúdo. Usou-se integralmente `docs/engineering/MASTER_ROADMAP.md` da base ativa como versão canônica efetivamente disponível. Não se usou `TESTE-main/aurora-skins-runtime` como ponto de partida. A ausência nessa busca não prova que o arquivo não exista em outro local ou serviço.

| Classe | Autoridade e uso nesta revisão |
| --- | --- |
| Original | Conteúdo do Master Roadmap no SHA acima. Preservado salvo alterações explicitadas; registro anterior não é teste novo. |
| Requisito V2 | Instruções explícitas de 29/09/2026 desta tarefa. Prevalecem sobre orientações anteriores conflitantes. Indexadas como `REQ-V2-*` na rastreabilidade. |
| Evidência estática | Leitura de manifests, lockfiles, matriz, regras e módulos identificados no inventário do backlog. Confirma texto/estrutura, não execução. |
| Evidência histórica | Resultados relatados em task files e matrizes anteriores. Continuam limitados ao SHA, hash, ambiente e cenário originais. |
| Proposta | ADR, contratos, limites, adaptadores e tarefas novos. Nomes de campos e caminhos propostos não são APIs existentes. |
| Pesquisa externa | Fontes oficiais consultadas em 29/09/2026; URLs e versões nos documentos de domínio. Não prova integração, gratuidade contratual de produção nem homologação Aurora. |
| Decisão pendente | Seleção final de biblioteca/versão, relay de distribuição, sinalização, versões de mods, APIs e critérios numéricos calibrados. Sem evidência suficiente, permanece bloqueada a capacidade afetada. |

O princípio de `AGENTS.md` — código/teste executado acima de intenção — vale para determinar estado real. Ele não torna um comportamento antigo superior a um novo requisito explícito: a divergência deve ser registrada e corrigida, sem falsificar o estado de implementação.

## Diagnóstico dos conflitos

| Assunto | Fonte anterior realmente lida | Correção V2 / consequência |
| --- | --- | --- |
| Relay gratuito | A proibição universal descrita pelo usuário não aparece no Master Roadmap/ADR-010 desta base. ADR-010 já previa fallback de terceiro e adiava relay próprio. | Não atribuir ao repositório uma regra que não foi encontrada. A regra vigente permite relay gratuito verificado, prefere direto e veda gasto obrigatório/automático. Nenhum teste deve rejeitar um caminho só por ser relay permitido. |
| Primeira tecnologia | Wave 6+ e ADR-010 priorizavam e4mc/World Host; playit/BYO constavam da decisão anterior. | Avaliar Iroh primeiro, comparar Quinn e preservar e4mc/World Host como referências/PoCs por mérito. Revisão explícita de estratégia, sem implementar três transportes ou obrigar conta/túnel externo no MVP. |
| UDP | O resumo R4 não definia serviços UDP nem retorno por participante. | Registro versionado, streams TCP e datagramas UDP têm contratos distintos. Testar retorno, isolamento, MTU, perda e carga simultânea. Escolher QUIC não captura o UDP dos mods. |
| Voz | Pesquisa histórica tratava Plasmo como “voz futura”; a retirada Companion preserva voz desktop. | Compatibilidade com Simple Voice Chat e Plasmo Voice entra explicitamente em R4; não criar chat de voz próprio. Voz de mods, TTS do Assistente e antigo Companion são capacidades distintas. |
| Endereçamento | Encaminhar pacotes sozinho não faz o mod descobrir a ponte do convidado. | Homologar endpoint real do host, anúncio do servidor e bind local de cada convidado. Anúncio global loopback não é solução universal, especialmente com jogadores diretos e via Aurora. |
| Companion/HUD | ADR-018 cancela o módulo; o plano paralelo ainda dizia que HUD começaria depois do harness. | Retirar essa instrução residual e reafirmar Assistente exclusivamente desktop; nenhum adaptador de rede reabre o escopo. |
| Estado das outras fases | Resumos antigos ainda chamam Doctor/Tools de ausentes ou protótipos; ADR-021/024/025 e matrizes posteriores registram incrementos. | Não rebaixar nem promover pela revisão R4. Ler task files atuais antes de reutilizar comportamento; implementação parcial não fecha CAS/restore/homologação. Registrar o descompasso histórico sem reescrever auditorias antigas. |
| Custo e produção | Gratuidade de software/serviço de teste não prova oferta sustentável para launcher público. | Lista de dependências exige operador, termos, autenticação, quotas, regiões, riscos e falha. Nenhum relay de produção é aprovado nesta entrega; modo direto continua previsto. |

As expressões antigas que proibiam qualquer relay não são regras desta entrega, inclusive no prompt e nos critérios de aceite. “Sem conexão direta” significa tentar somente alternativas explicitamente permitidas; falhar claramente quando nenhum caminho permitido existir. Relay gratuito pode ser caminho inicial e depois tentar direto se a biblioteca fizer isso; migração contínua precisa de teste.

## O que foi lido e o que se pode afirmar

Foram lidos integralmente o Master Roadmap canônico, `AGENTS.md`, `docs/ROADMAP.md`, a matriz ativa, o contrato IPC, a matriz de compatibilidade e os documentos de engenharia utilizados: revisão de arquitetura, pesquisa, resumo executivo, estratégia de teste/release, grafo, riscos, decisões, planos paralelo/integração e índices de ADR/tarefas/prompts. Foram lidas as ADRs 006, 007, 010, 012, 018, 023 e 025 e `SECURITY.md`; outros documentos e trechos de código utilizados estão identificados no inventário do backlog. Citar uma ADR adicional como leitura exigida para o próximo agente não afirma que ela tenha sido auditada nesta execução.

O inventário de código foi estático e focal: manifests/lockfile, declaração de módulos da engine, IPC, regras Firestore, transações e sessões. Não é uma auditoria completa de segurança do repositório, de todo Java, de infraestrutura implantada, de contas de provedores ou de todos os documentos históricos. A procura não encontrou Connect implementado nas fronteiras examinadas; Quinn transitivo no lockfile não prova transporte ativo. Não se executaram build, testes Rust/Java, emulador, Minecraft, rede externa, áudio, alterações de contas ou deploy. As únicas verificações desta entrega são documentais e Git.

Referências técnicas não garantem versões compatíveis com o Aurora. Iroh `1.1.0` é candidato documental condicionado a toolchain/build/licenças; APIs de voz citadas são referências de pesquisa. Nenhum artefato de mod foi instalado, redistribuído ou homologado.

## Preservação e fronteiras

R0 continua cobrindo Firestore, segurança, CI, licença, proveniência e distribuição pública. R1 mantém modularização, limites HTTP/IPC, Assistente desktop e homologação 9/9 Core/Skins. R1.5 mantém snapshots/rollback, configs e limites CurseForge. R2/R2.5 mantêm Doctor somente leitura, recovery, CAS, restore e Tools com plano/diff/confirmação/auditoria. R3 mantém manifesto/Creator. R4 mantém convites autorizados e sync por manifesto. R5 mantém produção, privacidade e zero-spend. Nenhum gate é encerrado por uma especificação ou por aprovação de PoC TCP.

Não se altera código, versão, matriz, JAR, licença, lockfile ou infraestrutura. Configurações de voz futuras precisarão de plano/diff, autorização apropriada, snapshot, recuperação e recusa de restauração destrutiva sobre edição concorrente. Fluxo final não pode exigir VPN externa manual, driver, administrador permanente ou portas manuais. Configuração explícita na PoC é desenvolvimento, não experiência final de um clique.

## Local da entrega

Branch documental `codex/aur-r4-connect-v2-docs`, worktree `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect-v2-docs`, baseada no SHA registrado. O recurso de worktree gerenciada do chat retornou “Not a git repository” porque o chat está em `TESTE-main`; foi usada worktree Git explícita no repositório Aurora, sem reutilizar ou modificar as outras worktrees.

O caminho canônico do roadmap foi revisado nessa branch; o checkout de origem não foi sobrescrito e não há cópia alternativa concorrente de roadmap nesta entrega. O [task file](../task-aurora-connect-v2-docs.md) lista exatamente os arquivos entregues e a validação. Não houve push, publicação, contratação, cobrança ou implementação.
