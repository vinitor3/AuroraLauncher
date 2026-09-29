# Aurora Connect — backlog incremental e homologação R4

Data: 29/09/2026. Backlog canônico reconciliado; objetivos e testes permanecem completos. O inventário e a matriz abaixo registram a preparação documental original; o [task de implementação](../../task-aur-r4-connect.md) registra o estado e os comandos executados do laboratório. Não reclassificar capacidades anteriores como ausentes nem tratar critérios T-* como provas já produzidas.

Base atualizada: `6a107b87adc4882bd464909e22650185f862ea5e`, launcher `0.1.1-alpha.14`, Core `1.1.5`, Skins `0.3.1`. A base inclui recuperação manual e cópias de diagnóstico; não começar da base histórica `aurora-skins-runtime`. Reconfirmar HEAD, mudanças locais, versões e colisões de IDs ao iniciar implementação.

Documentos de autoridade: [roadmap](../MASTER_ROADMAP.md), [especificação](../AURORA_CONNECT_SPEC.md), [proposta de ADR sem número reservado](../ADR/PROPOSAL-aurora-connect.md), [dependências e zero-spend](../AURORA_CONNECT_DEPENDENCIES.md), [diagnóstico/rastreabilidade/alterações](../../task-aur-r4-connect-v2-review.md) e [prompt Codex](../CODEX_PROMPTS/AUR-R4-CONNECT-V2.md). Requisitos explícitos da revisão V2 prevalecem sobre proibições antigas de todo relay.

## Estado da execução local — 29/09/2026

| Tarefa/capacidade | Estado comprovado | Trabalho ainda necessário |
| --- | --- | --- |
| 001 inventário/decisão | Reconciliados contratos/IDs/base; Iroh 1.3.0 fixado somente no laboratório | Gates de integração/distribuição, termos de qualquer infraestrutura futura |
| 002 TCP | **PROTÓTIPO** real autenticado: duplex, 8 MiB/contrapressão, half-close, concorrência, autorização e encerramento testados | Minecraft e redes externas; não concluir tarefa de produto |
| 003 UDP | **PROTÓTIPO** real: retorno isolado por peer/sessão/serviço, framing, MTU, abuso, expiração e carga TCP; downstream autorizado na biblioteca e CLI por plano/hash local | Integrar política apropriada ao adaptador, medir mod real e comprovar silêncio/keepalive/endereço; comando `host` convencional permanece 1:1 |
| 004 caminhos | Direto IPv4/IPv6 **loopback** testado; roteiro de dois computadores preparado | LAN, NAT/CGNAT, redes distintas, relay elegível e UDP bloqueado |
| 005 Minecraft/convites/UI | **PROTÓTIPO** de consulta STATUS/pong; lease/pin manual é mecanismo de laboratório | Servidor/JVM real, login/mundo, adaptadores, sinalização autorizada, fluxo social e estados reais |
| 006 Simple Voice Chat | **PARCIAL**: inspector/JAR Fabric 1.21.1/2.6.22 conferido; adaptador nativo de anúncio em cópia, confirmação e undo testados | Integração do caller com jogo encerrado/ponte, endereçamento efetivo, transporte/runtime e áudio próprios |
| 007 Plasmo Voice | **PARCIAL**: inspector/JAR Fabric 1.21.1/2.1.17 conferido; caminho resolvido e adaptador nativo em cópia com alteração/adição/undo testados | Gates separados de SVC, incluindo integração com ponte, lifecycle, dedicado/LAN e áudio |
| 008 robustez/liberação | Testes locais adversariais e rollback do laboratório | Matriz real, saturação prolongada, produto e todos os gates anteriores |

Baseline comprovado no [registro de comandos/resultados](../../task-aur-r4-connect.md): 24/24 testes Rust, cinco repetições de transporte com 55/55 resultados, fmt/clippy, 14/14 testes do inspector e smoke multiprocesso com 1 MiB TCP por direção/dez UDP em duas origens/1358 bytes negociados/limpeza aprovada. A falha anterior de UDP sob carga TCP foi preservada no histórico e corrigida com reserva de 16 KiB para datagramas no orçamento agregado. Isso não promete fairness universal nem perda zero. Critérios T-* abrangem mais cenários do que estes testes; nenhum é marcado integralmente concluído por semelhança de nome.

Incremento de política UDP após `b1c6871`, consolidado em `42bf0ad`: 34/34 Rust e fmt aprovados, com dez casos novos de downstream autorizado/QA. O host pode aprovar bytes/s e datagramas/s por serviço, compartilhados entre associações e persistentes na lease; o peer não altera essa política. Mantém socket conectado, inatividade e prazo absoluto. [Contrato canônico §5.1](../AURORA_CONNECT_SPEC.md#51-retorno-udp-autorizado--incremento-de-laboratório). Naquele snapshot o novo perfil estava na biblioteca/testes e a CLI mantinha o padrão anterior. Relay remoto continua não executado, por [gate específico de termos](../AURORA_CONNECT_DEPENDENCIES.md#dep-terms--limitações-das-fontes-e-decisões-pendentes), sem bloquear avanços locais.

Terceiro incremento comprovado: `plan`/`host-plan`, leitura local limitada e aprovação SHA-256 dos bytes exatos, sem escrita pessoal. Total 45/45 Rust (onze novos casos), fmt/clippy e smokes dos dois perfis aprovados. Downstream recebeu 8192 bytes/16 respostas após enviar 2 bytes de duas origens; padrão manteve 3138 bytes/dez UDP por direção; ambos com 1 MiB TCP por direção e limpeza aprovada. Hash zero recusado antes da identidade do host; fixture do plano inalterada. Tetos negociados 1358/1390 em execuções distintas permanecem observações dinâmicas. O [plano §3.1](../AURORA_CONNECT_SPEC.md#31-plano-local-revisado-por-hash--laboratório) não autoriza mudança de configuração dos mods ou uso de relay.

Quinto incremento: feature nativa `connect-voice-lab` desligada por padrão, sem comando Tauri/UI. SVC altera somente `voice_host` existente em cópia verificada após diff/hash/identidade/fingerprint/prazo; reutiliza proteção/journal/undo existentes. Quatro testes focados com artefato real e suites feature/padrão aprovados; padrão 258 PASS/cinco opt-in ignorados; fmt/clippy aprovados em ambas. QA corrigiu persistência antes da autorização e whitespace Unicode. Não conclui todos os critérios T-CONFIG ou células runtime. [Evidências e limites](../../aurora-connect-voice.md#adaptador-nativo-svc-em-cópia-de-diagnóstico--quinto-incremento).

Sexto incremento: caminho do Plasmo fixado resolvido em tag/JAR; adaptador TOML limitado em cópia, com seleção explícita de mod/caminho/hash. 12 testes focados com os dois artefatos reais; suites feature 268 PASS/sete opt-in ignorados e padrão 258 PASS/cinco opt-in ignorados; fmt/clippy aprovados. Limites da saída append também testados. Voz continua sem homologação e desligada. Auditoria desktop identificou vulnerabilidade rustls preexistente para correção pontual em incremento separado. [Comandos, riscos e evidências](../../task-aur-r4-connect.md).

Oitavo incremento: restauração seletiva do anúncio por recibo privado da aplicação, com nova aprovação e inventário atual. Preserva arquivos alheios modificados após aplicação, aceita limpeza após desabilitar o mod e recusa editar arquivo próprio divergente. Cancelar não publica ponto; erro após iniciar executor exige inspeção pela Recuperação. Recibo não sobrevive ao processo. Testes e limite de saída SVC no [domínio de voz](../../aurora-connect-voice.md#restauração-seletiva-do-anúncio--oitavo-incremento); não fecha os gates de crash/runtime/UI de T-CONFIG.

Nono incremento: defaults completos equivalentes reconstruídos das fontes, guardas de elegibilidade e recusa de hash alterado por reserialização TOML. Plasmo salva novamente o arquivo ao carregar; a limpeza pode exigir mescla explícita. Uma fixture com a mesma árvore não é roundtrip da JVM. Corrigida a fonte de defaults da tag divergente para branch fixada compatível com os campos do artefato, sem alegar build reproduzível.

Décimo incremento: consulta STATUS/pong limitada, com fixture independente através da ponte e CLI real; 56/56 testes Rust do laboratório passaram, incluindo onze novos. Não comprova servidor/JVM, login, mundo ou voz. Procedimento runtime permanece preparado e não executado.

Décimo primeiro incremento: plano separado para restaurar somente chaves próprias em arquivo regravado, com hash atual explícito e aprovação nova. SVC/Plasmo recusam conflito nas chaves; Plasmo valida árvore e preserva comentários ao retirar bloco criado. T-CONFIG-03 ganha provas locais de revisão/concorrência, mantendo pendentes caller com jogo parado, UI, roundtrip JVM, crash físico e recibo durável. Não amplia 006/007 a LAN/misto nem encerra os gates de voz. [Contrato](../../aurora-connect-voice.md#restauração-revisada-de-arquivo-regravado--décimo-primeiro-incremento).

Décimo segundo incremento: adaptador exige guard opaco com verificação de qualquer Java padrão no Windows ao adquirir lock, preparar e confirmar, inclusive após snapshot/CAS. Cobre o bloqueio conservador do dedicado `-jar`; não interpreta credenciais e não encerra processo. Caller de produto/UI continua pendente, assim como corrida com início externo e JVM renomeada/embutida. [Contrato](../../aurora-connect-voice.md#verificação-de-processos-no-adaptador--décimo-segundo-incremento).

Décimo terceiro incremento: T-LIFE-01/02 e T-CONFIG-03/04 ganham quatro cenários de entrega/retomada do lock com recibo em RAM, restauração estrita/revisada de ambos os mods e preservação de edições. Sem execução de Java; gates de runtime e recibo durável permanecem. [Evidência](../../task-aur-r4-connect.md#décimo-terceiro-incremento--entrega-e-retomada-do-lock-com-recibo-vivo).

Décimo quarto incremento: 005/008 têm helper funcional de plano/staging com nove pins oficiais, termos explícitos e downloads limitados testados contra HTTP local. Nenhuma JVM/Minecraft foi baixada ou executada; autorização de termos não foi inferida. Preparação não fecha T-MC-01; [procedimento](../../aurora-connect-runtime.md).

Décimo quinto incremento: 002/005/008 ganham guest-tcp e plano somente game, sem autorizar aux desnecessário. Smoke com dois processos atravessa STATUS/pong independente e recusa protocolo divergente; 59 Rust/dois Node aprovados. Contrato do plano canônico registra aux opcional; integração de runtime segue pendente. [Roteiro](../../aurora-connect.md#plano-explícito-de-serviços-do-laboratório).

Décimo sexto incremento: T-LIFE-01/02/008 têm supervisor funcional de filho próprio, 18 testes de saída/prazo/cancelamento/cleanup e ambiente mínimo. Não aprova artefato/termos/readiness nem executa Java; [contrato](../../aurora-connect-runtime.md#supervisor-de-processo-próprio--décimo-sexto-incremento).

Décimo sétimo incremento: prova Java 21 próprio + três consultas reais do probe nativo passou STOPPED→RUNNING→STOPPED. Minecraft ainda pendente, mas autorização EULA/MSA foi recebida; próxima preparação usará online-mode=false conforme preferência explicitada. [Evidência](../../task-aur-r4-connect.md#décimo-sétimo-incremento--probe-de-produção-com-java-próprio-real).

## 1. Evidência de partida e limites do inventário

Leitura local feita em 29/09/2026, sem build, lançamento do jogo, conexão externa ou teste runtime. A existência de um símbolo ou de um teste no arquivo não equivale à execução desse teste.

| ID de evidência documental | Material efetivamente examinado | Achado e limite |
| --- | --- | --- |
| INV-01 | `AGENTS.md`, HEAD, status Git, `scripts/active-mod-matrix.json` e `scripts/skins-feature-contract.json` | Branch/worktree exclusiva, ownership de monólitos, preservação dos nove pares e contrato cumulativo Core/Skins. Matriz mantém 18 JARs; não homologa Connect. |
| INV-02 | `package.json`, `apps/desktop/package.json`, trechos do `apps/desktop/package-lock.json`, `apps/desktop/src-tauri/Cargo.toml` e entradas pertinentes de `Cargo.lock` | Stack confirmada: React 19/TypeScript, Tauri 2, Rust. Manifestos alpha.14. Quinn `0.11.11` consta no lockfile e é referenciado pela entrada de `reqwest 0.12.28`; não consta como dependência direta do crate Aurora. Lockfile não comprova feature QUIC habilitada, uso por Connect, traversal ou transporte já implementado. Não executar atualização geral de dependências para esta PoC. |
| INV-03 | `apps/desktop/src-tauri/src/engine/mod.rs`, `engine/ipc.rs` (implementação), `engine/launch.rs` (preparação/launch), `launcher_sessions.rs`, `lib.rs`; trecho `launch_instance`/`launch_instance_tracked`/poll de `commands.rs` | IPC WebSocket local, nonce por execução, protocolo 1, filas limitadas, processo/instância e sessões de launcher independentes já têm contratos. IPC conectado não significa jogador dentro do mundo. Trecho examinado cria `LaunchIdentity::offline` e `IpcSessionProfile::offline`; futura auditoria deve identificar o fluxo online efetivo antes de prometer entrada automática em servidor oficial. Não reduzir `online-mode` para contornar isso. |
| INV-04 | Busca textual por `iroh`, `quinn`, `pion`, `quic-go`, `e4mc`, `World Host`, `UdpSocket` e `TcpListener` em Rust/desktop/Core; busca de integração de voz/LAN no Core API/runtime | Nenhuma implementação de ponte Connect foi localizada no escopo consultado. `TcpListener` encontrado em IPC e fixtures de download não é túnel multiplayer. Resultado de busca não é auditoria exaustiva do repositório nem prova de ausência em toda branch. |
| INV-05 | `docs/core-ipc.md`, roadmap completo e `docs/engineering/COMPATIBILITY_MATRIX.md` | Preservar Assistente desktop e retirada Companion/HUD; referência moderna Fabric/Forge 1.20.1 é candidata para priorização, não suporte de voz homologado. Histórico de Skins multiplayer não prova conectividade Connect. |
| INV-06 | `docs/instance-recovery.md`, `docs/diagnostic-lab.md`, ADR-027, ADR-028, `docs/task-aur-1167-doctor-lab.md`; lista de APIs/símbolos de `engine/recovery.rs` | Recuperação manual, CAS SHA-256/SQLite restrito, prévias, proteção anterior, journal, desfazer, observações Doctor e cópias de teste estão documentados como **IMPLEMENTADO MAS NÃO HOMOLOGADO**. Foram lidos os documentos e inventariados símbolos, não auditada integralmente a implementação. Reutilização exige avaliar os contratos reais. GC, snapshots automáticos gerais e backup/restauração de mundos não estão aprovados por esses recursos. |

Os registros de testes alpha.14 em AUR-1167 são evidência histórica daquele incremento, não testes novamente executados aqui. A correção documental não apaga nem substitui seus resultados. Caminhos completos são relativos à raiz do checkout; abreviações `engine/*`, `launcher_sessions.rs`, `commands.rs` e `lib.rs` usam o prefixo `apps/desktop/src-tauri/src/`, e `Cargo.lock` usa `apps/desktop/src-tauri/`. Itens novos estão explicitamente marcados **propostos**.

## 2. Regras comuns a todas as tarefas

- R4 continua P2/Wave 6+. Preservar gates R0 de distribuição, R1 de desktop/Core/Skins, R1.5 de updater reversível, R2/R2.5 de recovery/Tools e R3 de manifesto/Creator. PoC isolada autorizada para desenvolvimento não aprova liberação pública nem permite saltar gates.
- Caminho direto é preferido. Relay gratuito pode ser avaliado/habilitado apenas para uso cuja gratuidade, autorização, quotas, termos e ausência de cobrança automática estejam comprovadas. Opção de teste não é aprovação de distribuição. Não exigir VPS/relay próprio, VPN manual, driver ou modo administrador permanente. Sem rota permitida: erro claro. Nunca trocar silenciosamente para serviço pago.
- Autenticar e autorizar peer, sessão e serviço antes de abrir o destino do host; criptografia ponta a ponta também no relay. Convite expira, pode ser revogado e resiste a replay. A autenticação do túnel não substitui a de Minecraft ou dos mods. Não compartilhar tokens Microsoft/Minecraft entre peers/relays/logs.
- TCP transporta bytes por streams confiáveis bidirecionais, preservando ordem, integridade, backpressure, fechamento, cancelamento e isolamento. UDP mantém limites e conteúdo dos datagramas e retorno associado a participante/sessão/serviço; áudio não usa por padrão o stream ordenado do jogo.
- Endpoints do host são aprovados pelo responsável. Convidados pedem IDs de serviço, nunca IP/porta arbitrários. Metadados e manifestos não autorizam portas. Proibidos proxy aberto, SOCKS genérico, acesso geral à LAN, execução remota e exposição automática de RCON/SSH/administração.
- Simple Voice Chat e Plasmo Voice são integrações separadas de mods existentes. Não criar voz própria, gravação ou análise de áudio. Preservar identidade, autenticação e criptografia dos mods. Não exigir que ambos estejam ativos na mesma instância.
- Qualquer alteração de configuração requer plano/diff, autorização apropriada, snapshot válido, detecção de divergência e recuperação. Reutilizar contratos alpha.14 quando cobrirem o alvo; sua allowlist exclui mundos, `saves/**/serverconfig`, credenciais, módulos gerenciados e outros alvos. Não ampliar recovery silenciosamente nem ignorar seu filtro para fazer voz funcionar. Um adaptador novo de configuração precisa de escopo restrito e seus próprios testes se o executor atual não for adequado.
- Trabalhar em branch e worktree exclusivas. `App.tsx`, `commands.rs` e `apps/edge-proxy/src/index.ts` têm um único proprietário por wave; alocar esse proprietário antes de editar. Não sobrescrever mudanças locais, fazer reset destrutivo, push, publicação, ativação de faturamento ou geração de release como efeito colateral.
- Todo recurso sem evidência suficiente fica experimental e desativado para usuários por padrão. A aprovação de TCP não aprova UDP, voz, relay, multiplayer completo ou todos os nove pares. Estado muda por evidência registrada, não pelo fim de uma tarefa documental.

## 3. Sequência e dependências

IDs `AUR-R4-001` a `AUR-R4-008` são canônicos desta implementação, após comparar os pacotes de `d6f31db` e `5ac2f7e`. Não reservar um número ADR por inferência.

| Pacote alpha.13 / ID histórico | ID canônico alpha.14 | Significado preservado |
| --- | --- | --- |
| AUR-R4-001..005 | AUR-R4-001..005 | Inventário, TCP, UDP, redes, Minecraft/convites/UI |
| AUR-R4-006 / EV-R4-006-SVC | AUR-R4-006 | Simple Voice Chat |
| AUR-R4-006 / EV-R4-006-PV | AUR-R4-007 | Plasmo Voice |
| AUR-R4-007 / EV-R4-007 | AUR-R4-008 | Robustez/manifesto/liberação |

Referência histórica sem sufixo/SHA não autoriza reatribuir evidência. [Reconciliação completa](../AURORA_CONNECT_REVIEW.md) e [requisito → teste](../AURORA_CONNECT_TRACEABILITY.md). O arquivo alpha.13 `AURORA_CONNECT_BACKLOG.md` permanece histórico em Git; este é o único backlog ativo.

Local efetivamente escolhido para 001–003: `apps/connect-lab/`, crate Rust independente e experimental; os caminhos `engine/connect/` nas propostas de tarefa abaixo só se aplicam a futura integração autorizada. Não adicionar dependência/ponto de entrada no desktop para declarar uma PoC como produto. O pin e a decisão estão na ADR; testes/limites efetivos no task. Estado por incremento deve ser atualizado apenas com essa evidência, preservando os critérios não executados.

```text
AUR-R4-001 inventário/decisão/contratos
  -> AUR-R4-002 TCP autenticado
  -> AUR-R4-003 UDP e simultaneidade
  -> AUR-R4-004 redes/relay permitido
  -> AUR-R4-005 Minecraft/convites/UX
  -> AUR-R4-006 Simple Voice Chat (independente de 007)
  -> AUR-R4-007 Plasmo Voice (independente de 006)
  -> AUR-R4-008 robustez/manifesto/liberação
```

A ordem é ajustável ao bloqueio real. É útil investigar APIs/endereço dos mods e termos de relay já em 001; isso reduz retrabalho sem impor hooks à PoC TCP. 006 e 007 podem ser paralelas apenas com arquivos exclusivos e transporte estabilizado. Se a próxima auditoria encontrar TCP existente, não refazê-lo: reproduzir sua evidência e começar pelo próximo bloqueio. Não implementar três stacks concorrentes.

### AUR-R4-001 — Inventário, decisão arquitetural e contratos mínimos

| Campo | Contrato da tarefa |
| --- | --- |
| Objetivo | Selecionar uma stack principal verificável para a menor ponte autenticada e definir serviços, ameaças e custo sem fingir que uma pesquisa constitui implementação. |
| Escopo | Ler estado atual e todos os contratos impactados; avaliar Iroh primeiro, Quinn com descoberta/traversal compatíveis e Pion ICE + quic-go somente com vantagem concreta. Fixar versão selecionada e docs correspondentes; verificar plataformas/licenças/distribuição. Separar identidade/autorização, sinalização, descoberta, transporte, registro, adaptadores e UI. Definir limites explícitos e estados do backend. Auditar e4mc/World Host como candidatos de escopo específico, sem descartar por usarem relay nem inferir suporte UDP/voz. |
| Dependências | AGENTS, roadmap/gates, ADR proposta, inventário alpha.14 e matriz zero-spend. Nenhuma compra/infraestrutura necessária. Gates R0/R1/R3 continuam necessários à liberação apropriada. |
| Arquivos existentes | `AGENTS.md`; manifests/lockfiles root/desktop; `scripts/active-mod-matrix.json`; `engine/mod.rs`, `ipc.rs`, `launch.rs`, `launcher_sessions.rs`, trechos pertinentes de `commands.rs`; `docs/core-ipc.md`; recovery/diagnostic-lab e ADR-027/028. Ler regras/contratos Firebase/Worker antes de propor sinalização neles. Referência a arquivo adicional exige leitura efetiva. |
| Arquivos propostos | Documentação Connect desta revisão e, somente após escolher a stack, contratos de código em `apps/desktop/src-tauri/src/engine/connect/`. Esse diretório é proposta, não estrutura já existente. Sem novo helper Go por padrão. |
| Risco | Escolher pelo nome da biblioteca, confundir Quinn transitivo com transporte instalado, habilitar relay padrão sem verificar uso/custo, ampliar matriz ou recuperar Companion. |
| Testes | T-INV-01, T-SEC-01 a 05, T-COST-01 a 03 como casos definidos; inspecionar API da versão selecionada. Ainda não exigir homologação de rede para concluir apenas decisão/contratos. |
| Aceite | Documento reproduzível registra versão, licença, dependências auxiliares, mecanismo de autenticação/convite, contrato TCP/UDP e serviços aprovados, limites/estados, custos/termos datados e impedimentos. Se relay não for aprovado, seleciona modo direto e mantém falha explícita. Escolha principal e pendências estão separadas. Próximo incremento tem arquivos/comandos reais confirmados. |
| Rollback | Reverter somente alterações próprias da proposta/dependência experimental por diff revisado; preservar base e lockfiles anteriores, sem reset. Nada é ativado para usuários. |
| Estado/evidência | **NÃO IMPLEMENTADO** como incremento de produto; revisão e inventário INV-01..06 disponíveis. Avaliações e homologação de transporte: **NÃO VERIFICADO**. |

### AUR-R4-002 — Ponte TCP autenticada e reproduzível

| Campo | Contrato da tarefa |
| --- | --- |
| Objetivo | Demonstrar bytes TCP íntegros bidirecionais entre convidado e serviço explicitamente autorizado do host, com autenticação negativa verificável. |
| Escopo | Um listener loopback do convidado por serviço aprovado; conexão TCP local mapeada a stream confiável separado. Status Minecraft e sessão de jogo podem abrir conexões simultâneas. Autenticar transporte/convite, autorizar serviço antes do dial, copiar bytes com buffers limitados, backpressure, EOF/half-close conforme API, reset, timeout e cancelamento. Usar criptografia da biblioteca validando identidade esperada; nunca aceitar qualquer certificado. PoC pode trocar configuração explícita em desenvolvimento, identificada como tal. |
| Dependências | 001 com stack selecionada e limites. Não depende de Core/hook, conta cloud, mod de voz, UI final ou NAT real. Desenho/codec do controle reserva capacidades UDP sem implementá-las falsamente. |
| Arquivos existentes | `Cargo.toml`/`Cargo.lock`, `engine/mod.rs`, `lib.rs` apenas se exigidos pela forma de exposição escolhida. Não editar o monólito de comandos/UI para uma PoC de transporte sem necessidade. |
| Arquivos propostos | `engine/connect/mod.rs`, `engine/connect/services.rs`, `engine/connect/tcp.rs`, teste de integração `apps/desktop/src-tauri/tests/connect_tcp.rs`; são nomes a confirmar contra convenções reais, não arquivos preexistentes. Documentar como executar o harness real criado. |
| Risco | Proxy aberto, certificados sem validação, vazamento de credenciais, bloqueio de executor, cópia sem limite de memória e encerramento que perde bytes ou afeta outro stream. |
| Testes | T-TCP-01..05, T-SEC-01..05 e T-LIFE-01. Endpoints controlados em loopback/LAN; separar de T-NET. |
| Aceite | Hash/contagem dos bytes coincidem nos dois sentidos, incluindo escrita fragmentada, payload binário e cliente lento; conexões concorrentes não se misturam; cancelamento encerra recursos; destino não aprovado e peer inválido recebem zero bytes no endpoint. Teste produz evidência reproduzível e não transmite segredos em logs. Feature não aparece como pronta no launcher. |
| Rollback | Desabilitar/remover somente o ponto de entrada experimental; fechar listeners/streams e apagar somente dados efêmeros próprios. Não alterar mundos/configs da instância. Dependência nova é revertível por diff próprio. |
| Estado/evidência | **NÃO IMPLEMENTADO**; testes **NÃO VERIFICADO**. INV-04 não localizou ponte pronta; próxima auditoria pode substituir esta hipótese por evidência. |

### AUR-R4-003 — Ponte UDP bidirecional e uso simultâneo TCP/UDP

| Campo | Contrato da tarefa |
| --- | --- |
| Objetivo | Transportar datagramas preservados e entregar cada resposta ao convidado correto, enquanto streams TCP permanecem funcionais. |
| Escopo | QUIC DATAGRAM ou equivalente real da stack; associação autenticada por peer/sessão/serviço/cliente local. Usar socket UDP conectado dedicado por associação no host como hipótese simples, ou comprovar demultiplexação equivalente antes de compartilhar. Não encaminhar IP/porta fornecidos pelo convidado. Definir framing versionado, teto negociado de payload descontando overhead, MTU efetiva, tamanho de buffers, filas, frequência, banda e expiração. Estabelecer política por serviço; voz usa fila curta e descarta dados vencidos, UDP genérico não herda sem análise essa política. |
| Dependências | 001 e 002; API da versão fixada precisa suportar datagramas no caminho realmente usado. Biblioteca responde por criptografia/congestionamento. Comprovar mudança do limite de payload quando rota muda. |
| Arquivos existentes/propostos | Reusar módulo Connect proposto em 002; propostos `engine/connect/udp.rs` e `tests/connect_udp.rs`, contratos de capacidades/limites em `services.rs`. Não tocar mods ainda. |
| Risco | Respostas cruzadas, amplificação, buffers sem limite, datagrama truncado, áudio preso atrás do jogo, fragmentação ilimitada e associação residual autorizando replay. |
| Testes | T-UDP-01..09, T-MIX-01..03, T-SEC-02..05, T-LIFE-01/02. Medir payload real dos endpoints controlados e depois dos mods, sem gravar áudio. |
| Aceite | Limites/conteúdo e associação das respostas corretos para múltiplos participantes; datagrama acima do limite é rejeitado de modo observável, nunca truncado. Sem fragmentação implícita. Se necessária, fragmentação de aplicação tem aprovação específica, autenticação, limites de bytes/fragmentos e prazo de remontagem, testes de abuso e documentação; se não implementada, declarar incompatibilidade. Saturação respeita memória/banda e mantém evidência do impacto em voz/jogo. Sessão encerrada libera associações. |
| Rollback | Desabilitar capacidade UDP sem afirmar voz disponível; invalidar IDs/associações, fechar sockets e conservar TCP quando seguro. Não alterar o protocolo dos mods para compensar limitações. |
| Estado/evidência | **NÃO IMPLEMENTADO**; UDP/voz e simultaneidade **NÃO VERIFICADO**. TCP aprovado isoladamente não altera estes estados. |

### AUR-R4-004 — Conectividade real e relay gratuito verificado

| Campo | Contrato da tarefa |
| --- | --- |
| Objetivo | Medir caminhos reais entre redes e habilitar somente infraestrutura com uso/custo autorizado e observável. |
| Escopo | LAN, IPv6 com firewall, IPv4 alcançável, hole punching, NAT duplo/CGNAT. PCP/NAT-PMP/UPnP apenas se suportados, consentidos, temporários e limpos. Registrar descoberta, sinalização, STUN e relay separadamente. Se Iroh inicia por relay e busca direto, relatar essa sequência real e testar transição; preferência direta não obriga tentativa serial lenta. UDP externo bloqueado exige testar se a implementação selecionada usa relay sobre TCP/TLS/WebSocket e impacto no áudio. |
| Dependências | 001..003, matriz zero-spend preenchida; ambiente externo autorizado e sem custo. Registrar operador, autenticação, licença, termos, quotas, regiões, limites, cobrança e indisponibilidade. Um relay de teste não satisfaz gate de distribuição. |
| Arquivos existentes/propostos | Configuração do transporte e observabilidade do módulo Connect; propostos fixtures/configuração de desenvolvimento e roteiro `scripts/` somente se necessário e após confirmar nome. Arquivos Firebase/Worker somente após auditoria específica/ownership; não publicar infraestrutura nesta tarefa. |
| Risco | Relay padrão não autorizado, cobrança por excedente, software livre confundido com servidor grátis, exposição de IP, rotas falsas, afirmar CGNAT por timeout e dependência obrigatória de relay próprio. |
| Testes | T-NET-01..08, T-COST-01..03, T-MIX-02/03, T-SEC-05. Rede real com topologia registrada; loopback/mocks não passam nesses testes. |
| Aceite | Caminho observado vem da biblioteca/backend; custos e termos comprovados para uso definido; indisponibilidade/quota falham claramente sem opção paga automática. Caso nenhum relay sirva, entregar modo direto experimental e tabela de limites reais, mantendo avaliação de alternativa gratuita pendente. Sem promessa universal NAT/CGNAT, migração sem interrupção ou latência igual entre caminhos. |
| Rollback | Revogar config/credenciais efêmeras próprias, desligar relay afetado, remover mappings consentidos e encerrar sessões. Continuar apenas caminhos diretos autorizados; nenhuma ativação de faturamento. |
| Estado/evidência | **NÃO IMPLEMENTADO**; traversal/relay **NÃO VERIFICADO**. Referências oficiais em matriz de dependências são pesquisa, não homologação Aurora. |

### AUR-R4-005 — Minecraft, convites e estados reais no launcher

| Campo | Contrato da tarefa |
| --- | --- |
| Objetivo | Integrar sessão autorizada ao fluxo preparar → convidar → negociar → validar serviços → entrar, sem confundir túnel com jogo/voz prontos. |
| Escopo | Separar servidor público acessível, privado com agente autorizado, dedicado local e singleplayer aberto para LAN. Descoberta da porta, abrir LAN, transporte, auto-join e sync de modpack são capacidades distintas. Auto-join/abrir LAN exigem API/argumento/hook comprovado por versão. Criar convites autorizados vinculados a peer/chave/sessão/permissões, expiração/revogação/replay; sinalização mínima e retenção limitada por participante com regras testadas. Backend informa transporte/caminho, disponibilidade do servidor, ingresso real e voz separadamente. |
| Dependências | 001..004 para caminhos anunciados; auditoria auth/launch/IPC. R0 antes de distribuição; R1 antes de afirmar matriz/desktop completos; R3 para compatibilidade/sync por manifesto. Troca manual pode ser harness, não UX final de um clique. |
| Arquivos existentes | `commands.rs` e registro real dos comandos a localizar, `engine/launch.rs`, `engine/ipc.rs`, `launcher_sessions.rs`, React `App.tsx`, `docs/core-ipc.md`. Somente ler/adaptar Firebase/Worker após verificar contratos e quotas. `functions` continua legado. |
| Arquivos propostos | Módulo/tela de Connect fora de `App.tsx` quando viável; contrato de eventos/projeções públicas e comandos estreitos, nomes a fixar em inventário. Hook mínimo Core apenas se API real for insuficiente e com contrato/matriz explicitamente revisados. |
| Risco | Usar login Aurora como autenticação Minecraft; o fluxo launch examinado é offline. Não contornar `online-mode`, não enviar tokens a peers/relays. Status IPC é apenas IPC. Configuração de LAN não pode ampliar privilégios ou alterar outros módulos. Firestore não transporta jogo, voz ou modpack. |
| Testes | T-MC-01..05, T-SEC-01..05, T-UX-01..04, T-LIFE-01/02, T-SIGNAL-01/02. Validar sessões independentes/janelas e jogo fechado. |
| Aceite | Usuário distingue transporte, caminho, jogo e voz; cada rótulo cita produtor e evento comprobatório. Revogação encerra permissões/recursos; mundo encerrado resulta em estado real. Sem cooperação do host não se promete acesso privado. Falha de voz conserva jogo quando possível. Nenhum Companion/HUD/Assistente in-game. |
| Rollback | Desligar entrada experimental, revogar sessões, fechar recursos, restaurar somente config própria com comparação de estado. Preservar launcher normal, login lembrado/sessões independentes, Assistente e instalação de conteúdo. |
| Estado/evidência | **NÃO IMPLEMENTADO**; INV-03 comprova contratos existentes examinados, não integração Connect. Auto-join/LAN/convites/voz **NÃO VERIFICADO**. |

Avanço experimental em 005: produtor de consulta STATUS/ping e comando `minecraft-status` no laboratório, com limites, erros redigidos e resumo sem dados de jogadores/texto. Host/Guest reais encaminham a consulta até fixture independente; CLI também exercitada. Não conclui T-MC-01 de servidor Minecraft real, não faz login/auto-join/LAN, nem substitui os critérios de convites/interface desta tarefa. [Contrato delimitado](../AURORA_CONNECT_SPEC.md#91-produtor-experimental-de-status-tcp).

### AUR-R4-006 — Simple Voice Chat, isoladamente

| Campo | Contrato da tarefa |
| --- | --- |
| Objetivo | Homologar uma combinação explícita do mod existente, cobrindo encaminhamento UDP e endereçamento real do cliente. |
| Escopo | Selecionar versão exata/JAR/hash/licença/loader e ler a documentação/API dessa versão. Inspecionar configuração efetiva, bind do servidor, endpoint anunciado (incluindo investigação de `voice_host`) e ponte loopback por convidado. Descobrir porta real, conflitos/múltiplas instâncias, credenciais e payload máximo. Preferir configuração reversível ou add-on/API oficial; se necessário hook mínimo, verificar contrato real e escopo Core afetado. Não substituir bytes do fluxo Minecraft criptografado. |
| Dependências | 003 e 005; 004 para caminhos externos prometidos; decisão de endereçamento segura. Primeiro candidato é Minecraft 1.20.1 Fabric ou Forge já pertencente à matriz ativa, condicionado à disponibilidade da versão do mod. Não implica suporte aos outros sete pares. |
| Arquivos existentes/propostos | Adaptador proposto `engine/connect/adapters/simple_voice_chat.rs` se padrão Rust for adequado; configurações da instância somente após plano/diff. `engine/recovery.rs` e contratos alpha.14 como candidatos a reutilizar, sem presumir que aceitam o alvo. Add-on/hook Java, se necessário, recebe caminho proposto apenas após auditoria da API. |
| Risco | Anúncio global `127.0.0.1` quebrar clientes diretos/host sem ponte; perder edição concorrente; voz autenticar por identidade/IP de forma incompatível; reduzir segurança; redistribuir JAR sem autorização. |
| Testes | T-VOICE-01..07 exclusivamente com Simple Voice Chat; T-CONFIG-01..04, T-UDP-01..09, T-MIX-01..03 e T-MC-04. Servidor dedicado e LAN somente onde versão suportar; modo misto direto/Aurora obrigatório para anunciar essa capacidade. |
| Aceite | Duas pessoas em instâncias separadas trocam voz no mod real, sem gravação, com credenciais preservadas e endereçamento demonstrado. Registrar caminho, versões e payloads/limites. Diferenciar diagnóstico de tráfego UDP, estado autenticado do mod e validação funcional humana da voz. Restrição do modo misto ou LAN fica explícita se API/config global não os suporta. Falha da voz não encerra automaticamente o jogo. |
| Rollback | Remover somente adaptador/alteração autorizada, fechar pontes, restaurar diff se estado esperado coincide; divergência interrompe restauração para revisão. Não remover mod/edição anterior do usuário nem configurar servidor como inseguro. |
| Estado/evidência | **NÃO IMPLEMENTADO**; versão exata/API/endereço/runtime **NÃO VERIFICADO**. Fonte de pesquisa na especificação não homologa qualquer combinação. |

### AUR-R4-007 — Plasmo Voice, isoladamente

| Campo | Contrato da tarefa |
| --- | --- |
| Objetivo | Homologar Plasmo Voice por contrato próprio de configuração, autenticação e retorno UDP. |
| Escopo | Selecionar versão/JAR/hash/licença/loader e docs/API correspondentes. Investigar `[host.public]` na versão escolhida, sem assumir que é substituto universal de configuração por cliente. Separar bind, anúncio e ponte loopback; descobrir portas reais, conflito local e múltiplas instâncias. Mapear autenticação e efeito da mudança de IP aparente sem inventar encaminhamento de identidade. Configuração/add-on/API oficial preferidos; hook mínimo somente com evidência da necessidade. |
| Dependências | 003 e 005; 004 para caminho externo anunciado. Não depende de 006 nem exige ambos os mods instalados. Candidatos iniciais 1.20.1 Fabric/Forge da matriz, condicionados à versão real disponível. |
| Arquivos existentes/propostos | Adaptador proposto `engine/connect/adapters/plasmo_voice.rs`, fixtures próprias e config da instância revisada; reaproveitar abstração pequena de serviços/associações comprovada por 003. Recovery/config/hook têm as mesmas fronteiras explicitadas em 006, sem copiar configurações de outro mod. |
| Risco | Presumir semântica igual a Simple Voice Chat, tornar anúncio global incompatível com participantes diretos, sobrescrever config TOML do usuário ou alegar API por analogia. |
| Testes | T-VOICE-01..07 exclusivamente com Plasmo Voice; T-CONFIG-01..04, T-UDP-01..09, T-MIX-01..03 e T-MC-04. Registrar suporte dedicado/LAN e modo misto individualmente. |
| Aceite | Voz autenticada funcional entre dois clientes reais com retorno corretamente associado; destinos e portas observados; configuração reversível; segurança original intacta; matriz preenchida apenas nas combinações testadas. Se endereço por participante não for viável, manter modo afetado não suportado e não anunciar compatibilidade universal. |
| Rollback | Desabilitar apenas adaptador Plasmo, fechar associações e restaurar mudanças próprias sem sobrepor divergências; jogo continua quando possível. |
| Estado/evidência | **NÃO IMPLEMENTADO**; versão/API/configuração e execução **NÃO VERIFICADO**. Homologação de 006 não muda este estado. |

### AUR-R4-008 — Robustez, manifesto e liberação gradual

| Campo | Contrato da tarefa |
| --- | --- |
| Objetivo | Transformar capacidades demonstradas em oferta limitada, rastreável e reversível, sem extrapolar a matriz aprovada. |
| Escopo | Concluir cenários de rede/carga/falha, compatibilidade por manifesto R3 e sincronização autorizada via updater existente. IDs/proveniência/licenças/hashes continuam necessários; CurseForge sem download autorizado usa fluxo oficial. Validar limites de CPU/RAM/banda, reconexão, suspensão/retomada, encerramento, telemetria sanitizada e retenção. Registrar diferenças entre RTT, atraso de voz e desempenho do servidor; rollout experimental por capacidade/mod/caminho, com desligamento independente. |
| Dependências | 002..005; 006/007 somente para respectivas promessas. Gates R0, R1 e R3 preservados para liberação/sync; R1.5/R2/R2.5 para alterações/recovery efetivamente utilizadas; R5 privacidade/zero-spend contínuo. Não aguardar homologação de um mod para apagar o outro do backlog. |
| Arquivos existentes/propostos | `docs/engineering/COMPATIBILITY_MATRIX.md` quando runtime efetivamente mudar; task/evidências Connect; módulos Connect/UX; contratos reais de modpack/updater a auditar. Nenhuma substituição de matriz ativa, release histórico ou auto-instalador. |
| Risco | Vender NAT universal, chamar PoC de produto, downloads não autorizados, confundir restore parcial com backup de mundos, eliminar gates anteriores e ultrapassar quota com retry ilimitado. |
| Testes | Toda matriz aplicável, especialmente T-NET-07/08, T-MIX-01..03, T-LIFE-01/02, T-MAN-01/02 e T-REG-01/02. Registrar exceções e capacidades desligadas. |
| Aceite | Cada promessa pública mapeia combinação versão/loader/hospedagem/caminho testada e evidência; limites e falhas aparecem na UI/docs. Zero-spend efetivo revalidado; nenhuma cobrança automática. Métricas sem porcentagem de sucesso inventada. Matriz Core/Skins preservada; feature flag permanece desligada no que não foi validado. Liberação é tarefa posterior explicitamente autorizada. |
| Rollback | Desligar capacidades afetadas por versão/mod/caminho; revogar sessões; não forçar reinstalação nem remover mods/dados. Se houver release futura, usar processo reversível/assinado existente e preservar artefatos imutáveis. |
| Estado/evidência | **NÃO IMPLEMENTADO**; execução geral **NÃO VERIFICADO**. Esta revisão não faz rollout, build de release, push, publicação ou faturamento. |

## 4. Matriz de testes executáveis a especificar no código

Todos os testes abaixo têm execução **NÃO VERIFICADO** nesta revisão. Os identificadores são casos de aceite; não são nomes de comandos/suítes existentes. Criar ou reutilizar comandos reais durante cada tarefa, sem inventar scripts já disponíveis. Recursos externos só são usados em ambiente autorizado sem custo.

| ID | Caso e procedimento mínimo | Oráculo/evidência exigida | Tarefas |
| --- | --- | --- | --- |
| T-INV-01 | Conferir HEAD/status, manifests/lockfiles, nove pares, features Skins, módulos reais e IDs livres. | Lista de arquivos realmente lidos, versões/base, diff local preservado e escopo da auditoria. | 001 |
| T-SEC-01 | Peer correto/incorreto, chave trocada, certificado não esperado, convite expirado/revogado e replay. | Rejeição criptográfica/autorizações antes do dial; zero bytes no destino recusado; revogação encerra streams/associações vinculadas. | 001, 002, 005 |
| T-SEC-02 | Pedir serviço inexistente, protocolo trocado, outro participante/sessão e destino IP/porta arbitrário. | Host só conecta endpoint aprovado; TCP e UDP no mesmo número são registros/permissões distintos. | 002, 003, 005 |
| T-SEC-03 | Envelopes desconhecidos, oversize, flood de convites/streams/associações e replay de IDs expirados. | Limites versionados aplicados, memória/filas/CPU medidas, erro limitado e nenhum acesso geral à LAN/amplificação. | 002, 003, 005 |
| T-SEC-04 | Inspecionar listeners e conexão sem autenticação; tentar acesso de interface não loopback. | Listeners cliente somente loopback; nenhuma exposição de RCON/administração; bytes do host não encaminhados antes de autorização. | 002, 003, 005 |
| T-SEC-05 | Examinar logs/diagnóstico sanitizado e captura de tráfego sintético do túnel, inclusive relay. | Criptografia ponta a ponta entre agentes; sem tokens de conta, segredo global, conteúdo de áudio ou payload de jogo em logs; IPs redigidos no pacote compartilhado. Captura sintética não autoriza gravar voz. | 002..005, 008 |
| T-TCP-01 | Transferir bytes binários e tamanhos variados nos dois sentidos, com escritas pequenas/desiguais. | Hash e contagem exatos; ordem e conteúdo preservados. | 002 |
| T-TCP-02 | Vários clientes/streams, consulta de status e sessão de jogo concorrentes. | Cada resposta chega à conexão correta; fechar um stream não afeta os demais. | 002 |
| T-TCP-03 | Cliente lento, destino lento e escrita bloqueada. | Backpressure real, memória limitada, sem descarte silencioso de TCP e sem bloqueio da UI. | 002 |
| T-TCP-04 | EOF, half-close quando suportado, reset, timeout, cancelamento durante dial/cópia e desconexão do peer. | Fechamento definido do par, recursos liberados e dados confirmados preservados; erros sem hang. | 002 |
| T-TCP-05 | Destino fechado/recusado, porta ocupada no cliente e serviço retirado após convite. | Falha atribuída à etapa correta, sem retry infinito e sem escolher outra porta/destino sem informar a ligação nova. | 002 |
| T-UDP-01 | Enviar datagramas vazios, pequenos, binários e no teto permitido, nos dois sentidos. | Limites/conteúdo preservados; nenhum datagrama concatenado ou truncado. | 003, 006, 007 |
| T-UDP-02 | Dois ou mais convidados e serviços, inclusive clientes locais com endereços/portas semelhantes em máquinas distintas. | Associação peer/sessão/serviço/cliente inequívoca; respostas não cruzam jogadores. | 003, 006, 007 |
| T-UDP-03 | Injetar perda, duplicação e reordenação em harness controlado. | Ponte preserva semântica declarada sem transformar automaticamente UDP em entrega confiável ordenada; serviço tolera apenas o que realmente tolera. | 003 |
| T-UDP-04 | Variar MTU/payload máximo, overhead de framing e mudança de rota. | Teto dinâmico ou conservador documentado; rejeição explícita acima dele, contadores e limite de compatibilidade. | 003, 004 |
| T-UDP-05 | Quando fragmentação for aprovada, perder/reordenar/duplicar fragmentos, IDs concorrentes e expirados, tamanho falso. | Remontagem autenticada e limitada por participante/sessão, teto total, expiração e zero mistura; caso contrário registrar que fragmentação não existe. | 003 |
| T-UDP-06 | Saturar frequência/banda/filas, destinos sem resposta e respostas grandes. | Memória/CPU limitadas, rate limit e proteção contra amplificação comprovados; expiração limpa sockets/estado. | 003 |
| T-UDP-07 | Voz sintética de teste em fila com atrasos, sem gravação de áudio real. | Dados vencidos descartados conforme política do serviço, idade da fila observável; política não afeta UDP genérico indiscriminadamente. | 003, 006, 007 |
| T-UDP-08 | Spoof de remetente local/remoto, associação expirada, mudança de sessão e payload em serviço TCP. | Destino/retorno limitado ao endpoint autorizado e associação vigente; falha não autoriza redirecionamento. | 003 |
| T-UDP-09 | Encerrar/revogar sessão sob carga, reutilizar portas e reiniciar cliente. | Associações antigas não recebem nem transmitem; sockets/filas limpos sem atingir outra sessão. | 003, 006, 007 |
| T-MIX-01 | TCP e UDP simultâneos; depois jogo real e cada mod individualmente. | Integridade TCP, isolamento UDP, métricas por serviço e funcionalidade mantida sob carga definida. | 003, 006..008 |
| T-MIX-02 | Gerar carga TCP controlada e observar UDP; comparar uma conexão compartilhada e separação somente se necessária. | RTT, jitter, perda, idade da fila, CPU/RAM/banda registrados; decisão de isolamento motivada por dados, sem ganho prometido. | 003, 004, 006..008 |
| T-MIX-03 | Interromper serviço de voz/credencial/rota UDP com jogo ativo; restaurar voz. | Jogo não cai automaticamente; falha/reconexão de voz tem estado independente e ausência de associação residual. | 003..008 |
| T-NET-01 | Loopback e LAN autorizada, com topologia documentada. | Evidência local apenas; não marcada como travessia de NAT. | 004 |
| T-NET-02 | IPv4 alcançável e IPv6 direto, incluindo firewall bloqueando cada família. | Família/caminho observado e erro de etapa; IPv6 não presume firewall aberto. | 004 |
| T-NET-03 | Duas redes distintas, NAT duplo e CGNAT identificados por evidência de ambiente. | Resultado por cenário e caminho real; timeout isolado continua causa desconhecida. | 004, 008 |
| T-NET-04 | UDP externo bloqueado; testar relay permitido com transporte TCP/TLS/WebSocket se a versão o oferecer. | Suporte real da stack e retenção por retransmissões medidos; relay somente UDP não conta como solução. | 004 |
| T-NET-05 | Relay permitido ativo, desligado, indisponível e com quota; rota direta indisponível. | Falha clara, sem opção paga; rota/operador observáveis. Evento de quota só é afirmado com evidência própria, não inferido de timeout. | 004, 008 |
| T-NET-06 | Início por relay/transição direta ou mudança de rota que a biblioteca efetivamente suporte. | Sequência real, interrupções/perda/reconexão e continuidade TCP/UDP registradas; não presumir migração imperceptível. | 004 |
| T-NET-07 | Brasil–Brasil e Brasil–EUA, hosts autorizados e período de observação registrado. | Tempo até conexão, RTT, jitter, perda, banda/CPU/RAM e estabilidade, sem porcentagem geral derivada de amostra pequena. | 004, 008 |
| T-NET-08 | PCP/NAT-PMP/UPnP opcionais em roteador de teste que ofereça suporte. | Consentimento, validade, limpeza/expiração e cancelamento; ausência de suporte não dispara privilégio permanente nem afirma resolver CGNAT. | 004, 008 |
| T-COST-01 | Verificar fontes oficiais e configuração para cada operador externo. | Termos/licença/quota/região/uso/data registrados; opção gratuita para testes separada de distribuição. | 001, 004, 008 |
| T-COST-02 | Simular/exercitar limite permitido sem gerar cobrança; revisar hard cap do provedor. | Cobrança por excedente bloqueada efetivamente ou serviço desabilitado. Budget alert/contador local não aprovam o gate. | 001, 004, 008 |
| T-COST-03 | Indisponibilidade, abuso e tentativas repetidas de autenticação/sinalização. | Quotas/rate limits/backoff limitado e nenhum fallback pago; no MVP não se exige que usuário hospede relay. | 004, 005, 008 |
| T-MC-01 | Servidor público já acessível, sem agente host quando desnecessário. | Entrada normal preservada; não forçar túnel para servidor público nem alterar segurança oficial. | 005 |
| T-MC-02 | Servidor privado/dedicado com agente autorizado e destino aprovado. | Status e conexão separadas, autoridade do servidor preservada, host disponível e evidência real de ingresso. | 005 |
| T-MC-03 | Mundo singleplayer aberto para LAN, porta efetiva variável e mundo fechado novamente. | Capacidade de abrir LAN, descobrir porta, transportar e auto-join registrada separadamente; fechamento atualiza estado. | 005 |
| T-MC-04 | Dois jogadores/duas instâncias, com identidade oficial quando servidor requer; incluir direto + Aurora se anunciado. | Não mudar `online-mode`; handshake de mod/jogo válido, sessões isoladas; efeito do IP visto pelo servidor documentado. | 005..008 |
| T-MC-05 | Auto-join por versão e ação de usuário. | API/argumento/hook real identificado; autorização e rollback; IPC sozinho não comprova mundo carregado. | 005 |
| T-VOICE-01 | Inventariar individualmente Simple Voice Chat ou Plasmo Voice. | Versão/JAR/hash/loader/Java/API/docs/config efetiva/licença; outro mod ausente ou desativado no cenário. | 006, 007 |
| T-VOICE-02 | Observar bind, anúncio e ponte local de cada participante. | Endereço/porta correto por convidado, porta descoberta de fato, sem substituição global indevida por loopback. | 006, 007 |
| T-VOICE-03 | Dois clientes reais e servidor dedicado; depois LAN se suportada pelo mod/versão. | Voz funcional validada sem gravação; autenticação/criptografia original mantida e retorno isolado. | 006, 007 |
| T-VOICE-04 | Mod ausente, versão não homologada, credencial inválida e serviço parado. | Estados separados e mensagens baseadas em evidência; jogo continua quando possível. | 006, 007 |
| T-VOICE-05 | Cliente direto e cliente Aurora na mesma sessão se for modo anunciado. | Endereçamento funcional em ambos por mecanismo real; caso inviável, modo não aprovado explicitamente. | 006, 007 |
| T-VOICE-06 | Registrar tamanhos de datagrama e overhead sem armazenar conteúdo de voz. | Distribuição/teto observado compatível com limite de transporte; oversize conhecido não é escondido. | 006, 007 |
| T-VOICE-07 | Duas instâncias no mesmo convidado, porta ocupada e reconexão. | Pontes separadas, seleção reversível de endereço/porta e sem resposta para sessão antiga. | 006, 007 |
| T-CONFIG-01 | Criar plano/diff e snapshot de config já alterada pelo usuário; cancelar. | Cancelamento não escreve; escopo do snapshot cobre realmente o alvo, inclusive formato/caminho. | 006, 007 |
| T-CONFIG-02 | Aplicar, falhar durante escrita, reiniciar e restaurar. | Bytes anteriores recuperáveis; journal/atomicidade comprovados; instância fica bloqueada quando recuperação não é segura. | 006, 007 |
| T-CONFIG-03 | Editar a config entre plano/aplicação e entre aplicação/restauração. | Divergência exige nova revisão; edição concorrente nunca é sobrescrita silenciosamente. | 006, 007 |
| T-CONFIG-04 | Config sensível/excluída do recovery, mundo/serverconfig, links e caminho externo. | Recusa conservadora ou adaptador restrito aprovado; não expandir CAS/snapshot para incluir segredos ou mundos implicitamente. | 006, 007 |
| T-SIGNAL-01 | Participante indevido lê/escreve sessão, metadado oversize, convite expirado/revogado. | Regras negativas/positivas testadas, autorização por participante, expiração lógica e retenção limitada. | 005 |
| T-SIGNAL-02 | Inspecionar payloads e indisponibilidade da sinalização. | Apenas metadados necessários; nenhum áudio/jogo/modpack/segredo global; falha sem loops de custo. | 005 |
| T-UX-01 | Produzir eventos reais em cada etapa e atrasar/reordenar respostas. | UI não avança por timer nem confunde sessão antiga; transporte/caminho/jogo/voz independentes. | 005 |
| T-UX-02 | Simular porta ocupada, serviço indisponível, credencial/quota quando comprovadas e timeout genérico. | Mensagem correta e sem diagnóstico inventado de CGNAT; ações para recuperar/cancelar. | 005 |
| T-UX-03 | Encerrar jogo/host/launcher, sair convidado e revogar convite. | Backend reflete disponibilidade real, encerra recursos; não promete host 24 h. | 005 |
| T-UX-04 | Conferir cada rótulo público com a evidência produtora. | “Túnel conectado” exige transporte autenticado; “jogador no mundo” exige evento real compatível; “voz validada” exige nível declarado de prova, não bytes UDP apenas. | 005..008 |
| T-LIFE-01 | Cancelar cada etapa e finalizar processo, incluindo cliente lento. | Sem listeners, tasks, sockets/streams ou mapeamentos próprios vazados; outras sessões preservadas. | 002..005, 008 |
| T-LIFE-02 | Suspender/retomar, perda temporária da rede e chave/sessão revogada durante reconexão. | Reconexão limitada revalida autorização; estados/filas antigos não são reutilizados indevidamente. | 003..008 |
| T-MAN-01 | Manifestos compatível/incompatível/malicioso; hash errado e arquivo indisponível. | Manifesto não autoriza porta, execução remota ou instalação sem licença; diff/confirmar e updater real respeitados. | 008 |
| T-MAN-02 | Modpack parcialmente resolvido e CurseForge sem URL autorizada. | Fluxo oficial preservado, sem burla de origem ou sincronização de arquivos por Firestore. | 008 |
| T-REG-01 | Rodar checks relevantes reais e regressão Core/Skins se alterados. | Nove pares preservados, 18 JARs/contrato cumulativo intactos; build separado de homologação runtime. | 005..008 |
| T-REG-02 | Assistente desktop sem jogo, sessões independentes, updater/configs, recovery/Doctor/lab. | Nenhum Companion/HUD volta; funcionalidades alpha.14 preservadas, snapshots existentes não perdem escopo/segurança. | 005..008 |

Ao definir limites de uma tarefa, preencher valores concretos de sessões, streams, associações, mensagens, payload, memória, banda, frequência, timeouts e filas antes do teste. Valor proposto é hipótese de engenharia; só promover a limite suportado após medição. Não usar um único teto de latência como substituto de todas as métricas.

## 5. Matriz de compatibilidade candidata

Esta matriz registra combinações a investigar, **não suporte publicado**. A matriz ativa de Core/Skins permanece a fonte única dos nove pares. Nenhuma linha abaixo a amplia. Minecraft 1.20.1/Fabric 0.16.14 e Forge 47.4.23, Java 17, são candidatos iniciais já presentes nela; a versão exata dos mods de voz ainda deve ser selecionada de fonte oficial compatível. Não deduzir existência de um artefato de mod para um loader apenas porque o Aurora o suporta.

Coluna “caminhos” lista cenários de teste separados: D4 = IPv4 direto; D6 = IPv6 direto; N = NAT/hole punching comprovado; RG = relay gratuito autorizado para aquele uso; UB = UDP externo bloqueado, apenas se a stack oferecer caminho permitido compatível. Nunca marcar a linha inteira homologada após testar somente um caminho.

| ID | Serviço/mod | Versão do mod | Minecraft / loader candidato | Hospedagem | Caminhos a validar individualmente | Endereçamento / modo misto | Estado |
| --- | --- | --- | --- | --- | --- | --- | --- |
| C-TCP | Endpoint TCP controlado | `apps/connect-lab`, Iroh 1.3.0/lock próprio | Sem JVM/loader | Agente host + endpoint controlado | Loopback IPv4/IPv6 comprovado; LAN, D4/D6 externos, N, RG, UB pendentes | Listener loopback aprovado por sessão | PROTÓTIPO local; sem homologação externa |
| C-UDP | Endpoint UDP controlado | `apps/connect-lab`, framing 24 B | Sem JVM/loader | Agente host + endpoint controlado | Loopback IPv4/IPv6 comprovado; LAN, D4/D6 externos, N, RG, UB pendentes | Associação/resposta por participante/sessão/serviço testada; downstream limitado por plano/hash local | PROTÓTIPO local; perfis 1:1/downstream não aprovam voz |
| C-MC-P | Jogo público sem voz | Protocolo do jogo a registrar | 1.20.1 / Fabric 0.16.14 ou Forge 47.4.23 | Servidor público acessível | Rede normal, sem túnel obrigatório | Manter entrada/auth oficiais | Connect NÃO IMPLEMENTADO / cenário NÃO VERIFICADO |
| C-MC-D | Jogo por Connect | Protocolo do jogo a registrar | 1.20.1 / Fabric 0.16.14 ou Forge 47.4.23 | Dedicado com agente autorizado | LAN, D4, D6, N, RG, UB aplicável | Porta real do jogo; status e sessão distintos | NÃO IMPLEMENTADO / NÃO VERIFICADO |
| C-MC-L | Jogo por Connect | Protocolo do jogo a registrar | 1.20.1 / Fabric 0.16.14 ou Forge 47.4.23 | Singleplayer aberto para LAN | LAN, D4, D6, N, RG, UB aplicável | Descoberta/abrir LAN/auto-join separados | NÃO IMPLEMENTADO / NÃO VERIFICADO |
| C-SVC-FD | Simple Voice Chat | Pendente seleção oficial/hash | 1.20.1 / Fabric 0.16.14 | Dedicado | D4, D6, N, RG, UB aplicável | `voice_host`/API da versão; somente Aurora primeiro | NÃO IMPLEMENTADO / NÃO VERIFICADO |
| C-SVC-FL | Simple Voice Chat | Pendente seleção oficial/hash | 1.20.1 / Fabric 0.16.14 | LAN, se mod/versão suportar | D4, D6, N, RG, UB aplicável | Bind/anúncio/ponte separados; viabilidade pendente | NÃO IMPLEMENTADO / NÃO VERIFICADO |
| C-SVC-GD | Simple Voice Chat | Pendente seleção oficial/hash | 1.20.1 / Forge 47.4.23 | Dedicado | D4, D6, N, RG, UB aplicável | Config/API específica do artefato Forge | NÃO IMPLEMENTADO / NÃO VERIFICADO |
| C-SVC-GL | Simple Voice Chat | Pendente seleção oficial/hash | 1.20.1 / Forge 47.4.23 | LAN, se mod/versão suportar | D4, D6, N, RG, UB aplicável | Não herdar prova do Fabric | NÃO IMPLEMENTADO / NÃO VERIFICADO |
| C-SVC-M | Simple Voice Chat | Mesma versão explicitamente selecionada | Repetir por loader aprovado acima | Dedicado e LAN somente se anunciados | Cada caminho aprovado para Aurora + cliente direto | Misto requer API/config real por participante ou restrição explícita | NÃO IMPLEMENTADO / NÃO VERIFICADO |
| C-PV-FD | Plasmo Voice | Pendente seleção oficial/hash | 1.20.1 / Fabric 0.16.14 | Dedicado | D4, D6, N, RG, UB aplicável | `[host.public]`/API da versão; somente Aurora primeiro | NÃO IMPLEMENTADO / NÃO VERIFICADO |
| C-PV-FL | Plasmo Voice | Pendente seleção oficial/hash | 1.20.1 / Fabric 0.16.14 | LAN, se mod/versão suportar | D4, D6, N, RG, UB aplicável | Bind/anúncio/ponte separados; viabilidade pendente | NÃO IMPLEMENTADO / NÃO VERIFICADO |
| C-PV-GD | Plasmo Voice | Pendente seleção oficial/hash | 1.20.1 / Forge 47.4.23 | Dedicado | D4, D6, N, RG, UB aplicável | Config/API específica do artefato Forge | NÃO IMPLEMENTADO / NÃO VERIFICADO |
| C-PV-GL | Plasmo Voice | Pendente seleção oficial/hash | 1.20.1 / Forge 47.4.23 | LAN, se mod/versão suportar | D4, D6, N, RG, UB aplicável | Não herdar prova do Fabric | NÃO IMPLEMENTADO / NÃO VERIFICADO |
| C-PV-M | Plasmo Voice | Mesma versão explicitamente selecionada | Repetir por loader aprovado acima | Dedicado e LAN somente se anunciados | Cada caminho aprovado para Aurora + cliente direto | Misto exige mecanismo comprovado, separado de SVC | NÃO IMPLEMENTADO / NÃO VERIFICADO |
| C-REST | Outros sete pares Core/Skins atuais | Sem versão de voz selecionada | Somente pares existentes da matriz ativa | A decidir por mod | Nenhum caminho aprovado por esta revisão | Preservados; não afirmar suporte de voz/Connect | NÃO VERIFICADO |
| C-EXT | Outros serviços TCP/UDP | Seleção futura explícita | Matriz e licença a verificar | Destino aprovado por serviço | Somente após testes próprios | Novo adaptador/capacidade explícita, nunca proxy genérico | NÃO IMPLEMENTADO / NÃO VERIFICADO |

Fazer uma linha de evidência por combinação realmente executada, incluindo patch exato do loader e mod, Java, sistema operacional, host/convidados e caminho. Um mod aprovado em dedicado não aprova LAN; um loader aprovado não aprova outro; D4 não aprova relay/UDP bloqueado. Cada distribuição futura publica somente as linhas comprovadas, mantendo o restante experimental/desligado.

A [preparação posterior dos mods](../../aurora-connect-voice.md) selecionou para inspeção os candidatos Fabric 1.21.1 SVC 2.6.22 e Plasmo 2.1.17, dentro dos nove pares existentes. O quarto incremento conferiu os JARs oficiais locais, hashes publicados/recalculados, metadados e assinaturas de endereçamento sem executar bytecode. Os 31 testes Node finais incluem os dois arquivos reais, parser limitado, prova de artefato não forjável por JSON e planos derivados da versão efetiva. Nenhum JAR/loader/JVM foi executado; as linhas runtime 1.20.1 acima e a homologação 1.21.1 continuam pendentes. Transporte, endereçamento, instalação efetiva e voz audível são gates independentes.

## 6. Registro de evidência e métricas

Modelo a preencher em arquivo de evidência próprio **proposto** dentro da pasta de tarefas ou local protegido apontado pelo task file. Não armazenar capturas de voz, tokens, nonce/convites válidos, endereços públicos não redigidos, mundos ou dados de contas em Git.

| Campo obrigatório | Valor a preencher na execução |
| --- | --- |
| Identificador/data/responsável | `EV-R4-<tarefa>-<caso>-<execução>` proposto; UTC e fuso local quando necessário. |
| Base e ambiente | Branch, HEAD, diff próprio, launcher, stack/versão/lock hash, SO/arquitetura; ambiente autorizado sem custo. |
| Caso e combinação | T-* e C-* pertinentes; mod/loader/Minecraft/Java exatos, JAR/hash/origem/licença; dedicado/LAN/público/privado; dois mods nunca implicitamente ativos juntos. |
| Configuração e serviços | IDs/protocolos/capacidades e limites efetivos; bind/anúncio/ponte local redigidos; plano/snapshot/diff e estado de rollback. |
| Rede e dependências | Topologia/famílias/firewalls/NAT com fonte da classificação; operador/região/quota de relay e sinalização; fonte oficial/data/autorização de uso. |
| Procedimento reproduzível | Comandos realmente existentes/executados, flags sanitizadas, duração, tamanho/amostra/carga e pontos de coleta. |
| Resultado por etapa | Autenticação, transporte, caminho observado, serviço TCP, ingresso no mundo, UDP e voz separadamente; eventos produtores/erros confirmados. |
| Integridade e isolamento | Hash/contagem TCP sintético, tamanhos/delimitação UDP, associação de respostas, rejeições de permissões e limpeza observadas. |
| Medidas | Tempo de criar sessão até transporte, até jogo e até voz; RTT de rede; jitter e perda com método/unidade; CPU/RAM/banda e duração/estabilidade. Registrar atraso de áudio separadamente e somente por método autorizado sem gravação; se não medido, escrever NÃO VERIFICADO. MSPT/TPS ou desempenho do servidor exigem produtor real, não IPC removido do Companion. |
| Carga/limites | Sessões, streams, associações, bytes/pacote, filas/idade, MTU/payload negociado, frequência, rate limits, drops/oversize, saturação e concorrência. |
| Falhas e recuperação | Cancelamento, revogação, quota/relay, reconexão, divergência de config e rollback; falha observada versus causa ainda desconhecida. |
| Artefatos verificáveis | Caminhos reais de logs sanitizados/relatório/checksums, resultados completos e não só “passou”; sem conteúdo de áudio/jogo. |
| Estado e limites | CONCLUÍDO apenas no caso validado; IMPLEMENTADO MAS NÃO HOMOLOGADO quando código/teste local não prova runtime; NÃO VERIFICADO/PRECISA DE TESTE para lacunas. Sem extrapolação de loopback/LAN para NAT. |
| Impacto nos gates | Quais critérios específicos avançaram e quais permanecem pendentes; TCP isolado não conclui R4. |

Antes de comparar desempenho, fixar metodologia e amostra; distinguir RTT medido no transporte, tempo fim a fim até ouvir e saúde do servidor Minecraft. Não criar porcentagem de sucesso NAT, promessa de redução de ping ou prazo fechado sem dados. Diagnósticos compartilhados devem explicar limitações sem expor IPs/segredos.

## 7. Validação documental histórica e execução posterior

Na preparação documental original foi feito inventário read-only, conferência de base/matriz e criação deste backlog, sem execução dos casos T-* ou alteração de código/configuração de jogador. A implementação posterior está resumida no início e detalhada no task; não altera retrospectivamente o alcance daquela revisão. Nenhum mod/relay foi homologado. Os próximos incrementos preservam alpha.14, usam comandos reais adequados ao diff e anexam evidências; referências documentais não são atalhos para declarar implementação.
