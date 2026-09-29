# Aurora Connect — backlog incremental e matriz de validação

Data da revisão: **29/09/2026**. Base lida: `dc2ec5ac36a88f54b1130bac4789f8ecd56641d2`, launcher `0.1.1-alpha.13`, Core `1.1.5`, Skins `0.3.1`. Aurora Connect é nome de trabalho. Este documento é uma **proposta técnica**, não evidência de implementação ou homologação.

Relacionados: [roadmap canônico](MASTER_ROADMAP.md), [especificação](AURORA_CONNECT_SPEC.md), [ADR proposta sem número reservado](ADR/PROPOSED-aurora-connect-transport.md), [dependências e custo](AURORA_CONNECT_DEPENDENCIES.md), [rastreabilidade](AURORA_CONNECT_TRACEABILITY.md), [diagnóstico](AURORA_CONNECT_REVIEW.md) e [prompt executável](CODEX_PROMPTS/AUR-R4-connect-v2.md).

## Autoridade, estado e preservação

Os requisitos novos de Aurora Connect V2 prevalecem sobre documentos anteriores conflitantes: caminho direto preferido; relay gratuito permitido somente quando gratuidade, autorização e uso pretendido forem verificados; gasto obrigatório, faturamento automático e infraestrutura própria obrigatória continuam proibidos. TCP e UDP são capacidades explícitas. Integrar Simple Voice Chat e Plasmo Voice separadamente faz parte de R4; criar voz própria não faz parte do plano. Sem caminho direto e sem relay permitido disponível, falhar com diagnóstico sustentado por evidência.

A escala, as waves, os IDs e os gates do roadmap continuam válidos. R4 permanece P2/Wave 6+. Pesquisa e PoCs isoladas não fecham gates anteriores nem autorizam distribuição pública. R0 conserva segurança/Firestore, CI, licença, proveniência e release; R1 conserva modularização, limites HTTP/IPC, Assistente desktop e 9/9 runtimes Core/Skins; R1.5 conserva updater reversível, configurações, snapshot e regras de download; R2/R2.5 conservam Doctor somente leitura, CAS/restore e mutações confirmadas; R3 mantém manifesto/Creator; R5 mantém produção, privacidade e zero-spend. Sync por manifesto continua condicionado aos contratos e à reversibilidade, sem ampliar direitos de redistribuição.

Companion/HUD e Assistente in-game continuam retirados pela [ADR-018](ADR/ADR-018-remove-companion-keep-desktop-assistant.md). Um eventual adaptador de rede no Core não muda essa decisão. O servidor Minecraft continua autoritativo; agente do host autorizado e máquina disponível são necessários para compartilhar um destino privado. Não há hospedagem oficial 24 horas.

Todos os incrementos abaixo têm estado **NÃO IMPLEMENTADO** quanto ao trabalho novo especificado aqui e validação **NÃO VERIFICADO**. A busca focal não encontrou um transporte Aurora Connect, mas não constitui prova de inexistência em todo o histórico, outras branches ou módulos não lidos. Se a próxima auditoria encontrar implementação útil, conservar e verificar essa implementação, avançando ao próximo bloqueio real. Os rótulos operacionais seguem `AGENTS.md`: `CONCLUÍDO`, `PARCIAL`, `PROTÓTIPO`, `IMPLEMENTADO MAS NÃO HOMOLOGADO`, `NÃO IMPLEMENTADO`, `LEGADO`, `RISCO`, `HIPÓTESE`, `NÃO VERIFICADO` e `PRECISA DE TESTE`.

## Inventário estático efetivamente realizado

Nenhum build, teste de unidade, emulador, Minecraft, endpoint externo ou ensaio de rede foi executado nesta revisão. A inspeção distingue leitura de código de leitura documental; listas de testes existentes não significam testes aprovados agora. Caminhos nesta seção são reais e relativos à raiz do checkout indicado acima.

| Evidência de leitura | Fonte e trecho/símbolo lido | Achado e limite |
| --- | --- | --- |
| EST-01 | `AGENTS.md`; `git status --short`; `git rev-parse HEAD`; `scripts/active-mod-matrix.json` | Checkout limpo no início da inspeção; base e versões acima; nove targets exatos abaixo. Não demonstra homologação runtime. |
| EST-02 | `docs/engineering/MASTER_ROADMAP.md`, `DEPENDENCY_GRAPH.md`, `ARCHITECTURE_REVIEW.md`, `COMPATIBILITY_MATRIX.md`; ADR-007, ADR-010, ADR-018, ADR-023 e ADR-025; `docs/core-ipc.md`; `CODEX_TASKS/AUR-R1-001-network-limits.md` | Gates, ownership e contratos preservados. ADR-010 já admite fallback terceiro e adia relay próprio. A matriz registra histórico com limitações; relato de multiplayer/Chat Heads não homologa voz ou o transporte proposto. |
| EST-03 | `package.json`, `apps/desktop/package.json`, cabeçalho de `apps/desktop/package-lock.json`, `apps/desktop/src-tauri/Cargo.toml`; entradas pertinentes de `Cargo.lock` | Stack real: Rust/Tauri v2 e React 19/TypeScript. `Cargo.toml` declara Tauri `2.11.1`, `tungstenite 0.24`, `reqwest 0.12` com `blocking/json/rustls-tls`; não declara `iroh` ou `quinn` como dependência direta. |
| EST-04 | `Cargo.lock`: `quinn 0.11.11`, `quinn-proto 0.11.17`, `quinn-udp 0.5.15`, lista de dependências de `reqwest 0.12.28` | Quinn aparece na resolução transitiva de reqwest. Sua presença no lockfile não prova feature QUIC ativa nem implementação de ponte multiplayer. Uma nova escolha deve ter dependência explícita e versão/API verificadas. Iroh `1.1.0` é candidato pesquisado nos documentos desta revisão, não pacote instalado aqui. |
| EST-05 | `apps/desktop/src-tauri/src/engine/mod.rs`; busca focal por Connect, multiplayer, social, Iroh e Quinn em `apps/desktop/src-tauri/src` e `apps/desktop/src` | Há módulos de launch, IPC, updater, Doctor, Core/Skins e Tools; não foi encontrado um módulo de transporte Connect nesse escopo. Categorias de catálogo chamadas multiplayer/social e retransmissão de skins não são evidência desse transporte. |
| EST-06 | `apps/desktop/src-tauri/src/engine/ipc.rs`, especialmente `IpcServer::start_with_session`, `handle_connection`, `retired_companion_message` | WebSocket em `127.0.0.1:0`, nonce por execução, filas de 64, autenticação `hello`/nonce/protocolo 1 antes dos eventos, rejeição de mensagens retiradas e payloads com campos sensíveis. Esse contrato local não é autenticação remota Connect. |
| EST-07 | Mesmo arquivo: `accept_connections` e início de `handle_connection` | Observação para R1: `tungstenite::accept(stream)` precede `set_read_timeout`; o prazo de 5 s da mensagem `hello` começa depois do handshake WebSocket e o atendimento é serial. Não foi reproduzido ataque nem executado teste; não considerar todo o gate HTTP/IPC fechado. A revisão documental não altera esse código. |
| EST-08 | `apps/desktop/src-tauri/src/engine/content_updates.rs`: tipos públicos; `preflight_transaction`, `commit_one`, `rollback_committed`, `restore_snapshot`, `restore_snapshot_file`, `acquire_instance_mutation_lock` | Há padrões concretos de confinamento, hash, staging/rename, journal e bloqueio por instância. O snapshot de modpack e a troca de conteúdo não são automaticamente uma transação segura de configuração de voz; reavaliar semântica e concorrência antes de reutilizar. |
| EST-09 | `apps/desktop/src-tauri/src/engine/ai_tools.rs`: `AiToolBroker::prepare`, `confirm`, `prepare_undo`, `proposed_change`, `apply_undo` | Planos com expiração/digest, vínculo a janela/instância, bloqueio e revalidação; undo verifica `after_hash` e recusa alteração posterior. Suporte atual é a remoção restrita de mod e opções JSON booleanas/numéricas permitidas, não editor genérico de TOML/properties ou endereços de voz. |
| EST-10 | `apps/desktop/src-tauri/src/launcher_sessions.rs`: `LauncherSessions::acquire`, `launcher_auth_scope` | Sessões independentes por processo/janela com bloqueio do sistema operacional; reutilizar a fronteira de instância, sem tratar esse identificador local como identidade criptográfica de peer. |
| EST-11 | `firebase/firestore.rules`, lido integralmente | Regras definem `users`, `publicProfiles` e `usernames`; não foi encontrado contrato de convites/sinalização nesse arquivo. Acrescentar metadados de sessão exige projeto de regras e teste próprio; não usar banco como relay. |
| EST-12 | `.github/workflows/ci.yml`, `apps/edge-proxy/package.json`, `functions/package.json`; scripts dos manifests acima | Comandos reais disponíveis listados ao final. CI lida não executa todos os checks do `AGENTS.md`. Não existe `firebase/package.json` no checkout inspecionado; não inventar um comando `npm test` para regras. |

Esta inspeção não leu todo o monólito `commands.rs`, todos os adaptadores Java, toda a implementação do Worker, todo o lockfile ou todas as branches. A busca nesse monólito localizou símbolos de launch, updater, IPC e Tools, mas isso não substitui leitura dos corpos que uma implementação vier a alterar. A revisão futura precisa inventariar os trechos afetados antes de editá-los.

## Coordenação, dependências e evidências

A busca no checkout não encontrou IDs `AUR-R4-001` a `AUR-R4-007` previamente atribuídos. Eles são propostos abaixo, sem renumerar tarefas existentes. Cada tarefa mantém detalhes neste documento, evitando sete task files duplicados; registros de execução podem ser acrescentados à mesma seção ou a uma evidência específica necessária.

Nenhum agente edita simultaneamente `App.tsx`, `commands.rs` ou `apps/edge-proxy/src/index.ts`. Propriedade exclusiva por arquivo deve ser registrada antes de uma wave; `Cargo.toml`, `Cargo.lock`, registro de módulos e regras compartilhadas também precisam de coordenação. Mudanças Java são isoladas e builds Core/Skins sequenciais. Não ampliar `functions` legado sem ADR. Não misturar refatoração ampla ao incremento de transporte.

Ordem proposta: `001 → 002 → 003 → 004 → 005 → 006 → 007`. A investigação de endereçamento dos mods em 006 começa durante 001, para detectar impossibilidades cedo. A prova de NAT de 004 pode preceder a conclusão de 003 se reduzir incerteza, mas não aprova UDP/voz. A PoC de transporte pode usar endpoints e chaves de desenvolvimento explícitos; essa configuração não é a UX final. O grafo do roadmap não é reescrito por esta ordem: integração/distribuição depende dos gates aplicáveis de R0/R1; sync depende de R1.5/R3 e recuperação relevante.

Para cada execução registrar: ID, data, commit/base, versão do binário/transporte/OS, arquivos alterados, ambiente autorizado, comandos reais, resultado bruto sanitizado, hash dos artefatos, cenário, medição, limitações, custo observado, rollback e estado. Identificadores `EV-R4-001` a `EV-R4-007` abaixo são **evidências futuras sem arquivo produzido**, não resultados. Um relatório estático usa `EST-*`; um teste só recebe aprovação depois de execução verificável.

## AUR-R4-001 — Inventário, decisão, ameaças, custos e serviços

- **Fase/prioridade:** R4/P2; descoberta técnica antes de habilitar feature.
- **Objetivo:** reduzir as incertezas que impedem escolher um transporte e implementar uma ponte estreita.
- **Escopo:** revalidar AGENTS, status, HEAD, matriz e dependências; procurar implementação anterior; avaliar primeiro Iroh e comparar Quinn com NAT traversal, e Pion ICE + quic-go apenas se vantagem concreta justificar processo auxiliar. Fixar versão e API, plataformas, manutenção, licenças e distribuição. Separar identidade/autorização, sinalização, descoberta, traversal, relay, transporte, serviços, adaptadores e UI. Produzir threat model e contrato versionado pequeno; host autoriza destino, convidado pede `serviceId` autorizado. Delimitar TCP e UDP, limites de associação/datagrama/filas/timeout e perfis de serviço. Investigar Simple Voice Chat e Plasmo Voice individualmente, incluindo endereço anunciado e bridge por convidado, antes de prometer suporte.
- **Dependências:** os documentos desta revisão; acesso ao checkout corrente; fontes oficiais e ambiente de teste sem custo. A conclusão documental não substitui gates R0/R1 nem ADR aceita.
- **Arquivos:** reais: `AGENTS.md`, `scripts/active-mod-matrix.json`, manifests/lockfiles, documentos e módulos EST-01..12. Novos caminhos de código permanecem **propostos**, sem criação nesta revisão. Atualizar a ADR proposta somente com decisão efetiva; não reservar número sem verificar o índice de ADRs.
- **Risco:** adotar API de versão errada; usar dependência transitiva como implementação; assumir que open source implica serviço gratuito; criar segredo global no cliente; confundir relay de skins com transporte Connect.
- **Teste/validação planejada:** conferência documental de versões/termos; protótipo mínimo da API escolhida apenas no incremento autorizado seguinte; rastrear cada requisito para teste observável; revisão independente das ameaças e da política de custo. Conferir cenário UDP bloqueado e caminho efetivo TCP/TLS/WebSocket quando existir, sem presumir suporte.
- **Critério de aceite:** escolha principal justificada com alternativas e pendências; contratos testáveis TCP/UDP; registro completo de operadores/quotas/licenças/termos/regiões/autenticação/falhas/cobrança; nenhum relay habilitado sem aprovação técnica/contratual do uso pretendido. Dependência de produção desconhecida fica bloqueada, mantendo direto possível.
- **Rollback:** documentos versionados e decisão reversível; nenhuma conta, serviço ou configuração de jogador alterada.
- **Estado real/evidência:** revisão documental e inventário estático **PARCIAL** (`EST-01..12`); incremento executável **NÃO IMPLEMENTADO**. `EV-R4-001` pendente: decisão aceita, contrato final e revisão de ameaças/custos. Não há teste de transporte aprovado.

## AUR-R4-002 — Menor ponte TCP autenticada verificável

- **Fase/prioridade:** R4/P2, PoC experimental desativada para usuários por padrão.
- **Objetivo:** atravessar bytes entre um listener local e um destino TCP aprovado com autenticação criptográfica e isolamento de conexões.
- **Escopo:** uma implementação de transporte escolhida em 001, sem implementar todas as alternativas. Listener do convidado em loopback; canal autenticado ponta a ponta; host valida peer, sessão, expiração e permissão antes de conectar ao endpoint da allowlist. Uma conexão TCP de aplicação corresponde a stream confiável independente, com ordem, integridade, bidirecionalidade, backpressure, half-close quando suportado, fechamento e cancelamento definidos. Status e jogo não compartilham o mesmo stream. Nunca enviar TCP em datagramas sem confiabilidade. Rejeitar destinos arbitrários, serviço desconhecido e protocolo divergente. Especificar e testar o contrato de capacidade UDP/negociação e rejeição de datagrama inválido sem afirmar que forwarding UDP está implementado.
- **Dependências:** 001; biblioteca com versão fixada/API verificada; endpoint controlado local. Em desenvolvimento pode haver troca explícita de identidade/chave pública por canal confiável; isso não substitui convites de produto. Não aceitar qualquer certificado ou reutilizar nonce do IPC como autenticação remota.
- **Arquivos:** reais candidatos: `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/Cargo.lock`, `apps/desktop/src-tauri/src/engine/mod.rs` e `apps/desktop/src-tauri/src/lib.rs` apenas se o registro necessário exigir. **Propostos:** `apps/desktop/src-tauri/src/engine/connect/` com unidades pequenas de autorização, serviços e ponte; fixture/harness de teste junto ao módulo ou em `apps/desktop/src-tauri/tests/` (nomes finais a confirmar). Não tocar em Core, App.tsx, Worker ou contas de produção na PoC mínima.
- **Risco:** proxy aberto, conexões sem dono, memória ilimitada, encerramento que trava, identidade do canal desvinculada da autorização, confundir loopback com NAT traversal.
- **Testes planejados:** bytes arbitrários incluindo fluxos maiores que buffers; envio/retorno simultâneo; várias conexões independentes com payloads identificáveis; leitura lenta/backpressure; EOF/half-close, cancelamento, queda abrupta e limpeza; identidade incorreta, convite de desenvolvimento expirado/revogado/repetido, sessão cruzada, serviço/destino não autorizado e limites de streams/memória. Não depender de Minecraft para validar transporte básico.
- **Critério de aceite:** duas pontas autenticadas encaminham bytes exatos para somente o serviço autorizado; negativas não entregam bytes ao endpoint do host; recursos e handles retornam ao limite esperado após cancelamento; erro público sem segredos. Contrato UDP já inclui limites, associação e testes de framing previstos, com capacidade operacional ainda indisponível. Relatório rotula o ambiente real: loopback, duas máquinas LAN ou redes distintas.
- **Rollback:** interromper sessão, cancelar streams, fechar listeners e remover apenas recursos efêmeros próprios; feature continua desativada. Reverter o incremento por commit sem reset destrutivo; não apagar mudanças locais.
- **Estado real/evidência:** **NÃO IMPLEMENTADO**, `EV-R4-002` pendente com comandos/logs sanitizados e resultados positivos/negativos. Aceitar TCP não aprova UDP, voz, relay, NAT nem R4 completa.

## AUR-R4-003 — Ponte UDP e concorrência com TCP

- **Fase/prioridade:** R4/P2, experimental.
- **Objetivo:** preservar datagramas e entregar cada resposta ao participante, sessão, serviço e associação corretos.
- **Escopo:** listener UDP loopback por ponte/convidado; associação autenticada e limitada com identidade local não reutilizável entre sessões. No host, preferir socket conectado distinto por associação para retorno inequívoco, ou justificar/testar outra demultiplexação. Datagramas preservam conteúdo e fronteiras; usar QUIC DATAGRAM/capacidade equivalente real. Não colocar áudio no stream confiável do jogo. Consultar máximo negociado/atual, descontar overhead e recusar excesso com erro/contador controlado, nunca truncar. Sem fragmentação na primeira ponte; se mod exigir maior tamanho, declarar incompatibilidade até projeto limitado de fragmentação autenticada/remontagem com expiração e teste. Filas e expiração são por perfil: voz pode descartar vencidos em fila curta; outros protocolos UDP não herdam essa política sem análise.
- **Dependências:** contrato 001 e ponte 002; API de datagramas verificada na versão escolhida; endpoints UDP controlados. Mesmo número TCP/UDP representa serviços distintos. Regra de retorno considera que o serviço verá o endereço/porta do proxy, não o IP original; não inventar identidade que o servidor não suporte.
- **Arquivos:** módulo `engine/connect/` e fixtures **propostos** em 002; contrato de serviços e especificação reais criados nesta revisão. Não exige hook Core nem instalação de mod de voz para provar forwarding.
- **Risco:** resposta cruzada, spoofing, amplificação, associação vazada, saturação/memória, MTU/path menor após mudança de rede, contenção entre jogo e voz.
- **Testes planejados:** dois participantes × dois serviços com respostas únicas; vários sockets locais/instâncias; rejeitar sessão/peer/serviço errado; datagramas vazios quando protocolo permitir, limites de tamanho e um byte acima; perda, duplicação e reordenação controladas; expiração/reabertura e IDs antigos; fila cheia, limite de frequência/banda e CPU/RAM; mudança de máximo do caminho; simultâneo TCP pesado + UDP bidirecional; shutdown completo. Inspecionar buffers para garantir que receive não trunca silenciosamente.
- **Critério de aceite:** nenhuma mistura de jogadores/serviços, payload/fronteiras íntegros para tamanhos suportados e retorno correto; excesso/expiração registrados sem conteúdo; consumo limitado; carga TCP não paralisa controle/cancelamento e seus efeitos sobre UDP são medidos. Resultados de endpoints sintéticos não homologam mod de voz.
- **Rollback:** cancelar associações, fechar sockets e esvaziar filas; manter jogo independente quando somente UDP falhar; marcar serviços UDP indisponíveis sem promover para voz validada.
- **Estado real/evidência:** **NÃO IMPLEMENTADO**, `EV-R4-003` pendente. A PoC TCP não é evidência deste incremento.

## AUR-R4-004 — Redes distintas, caminhos observáveis e relay permitido

- **Fase/prioridade:** R4/P2, sem ativação de faturamento.
- **Objetivo:** medir conectividade efetiva além de loopback/LAN e identificar exatamente a infraestrutura de cada caminho.
- **Escopo:** LAN, IPv6 com firewall, IPv4 alcançável e hole punching quando o transporte suportar; PCP/NAT-PMP/UPnP opcionais somente com consentimento, duração limitada e limpeza. Não presumir que mapeamento doméstico contorna CGNAT. Usar os mecanismos da biblioteca escolhida sem impor ICE/STUN redundante. Se alternativa ICE + QUIC for escolhida, demonstrar reaproveitamento do caminho e coordenação de leitura/demultiplexação do socket. Relay gratuito somente quando verificado para o uso atual; teste não autoriza distribuição. A biblioteca pode começar por relay autorizado e tentar direto, registrando transições reais. Não prometer migração sem interrupção antes de medir.
- **Dependências:** 001/002 e preferencialmente 003; operadores/termos/custos registrados; máquinas e redes autorizadas sem custo; nenhum provedor com cobrança excedente sem bloqueio efetivo comprovado. Se não houver relay permitido, testar direto e documentar falha fora do alcance, sem substituição paga.
- **Arquivos:** transporte/telemetria local no módulo **proposto**; [registro de dependências](AURORA_CONNECT_DEPENDENCIES.md), ADR e spec reais. Configurações de teste novas são **propostas**, não credenciais embutidas. Cloudflare/Firestore existentes não se tornam relay de tráfego.
- **Risco:** permissão de desenvolvimento tomada como produção; UDP bloqueado com relay UDP igualmente inacessível; leak de IP/segredo nos logs; mapeamento que permanece aberto; quotas compartilhadas imprevisíveis.
- **Testes planejados:** redes distintas IPv4/IPv6; NAT duplo/CGNAT observado sem deduzir topologia de um timeout; UDP externo bloqueado; relay indisponível/quota; autorização recusada; mudança/perda de caminho; medir TCP/UDP simultâneos. Quando existir fallback TCP/TLS/WebSocket, verificar suporte na biblioteca/serviço e medir impacto de retransmissões/atraso na voz; não anunciar latência equivalente. Testar Brasil–Brasil e Brasil–EUA quando houver hosts autorizados e registrar locais gerais sem expor IPs.
- **Critério de aceite:** cada ensaio identifica direto/relay/misto/desconhecido segundo telemetria confiável, operadores envolvidos, capacidade TCP/UDP, falha e limites; ausência de gastos obrigatórios/automáticos comprovada. Só declarar NAT atravessado com rede distinta e evidência do caminho. Produção por relay permanece desabilitada se autorização/quota/risco forem pendentes.
- **Rollback:** cancelar sessões e mapeamentos próprios, respeitar TTL/cleanup, revogar credenciais de teste próprias se aplicável, desabilitar provedor falho e manter direto; sem contratação, deploy ou alteração de billing.
- **Estado real/evidência:** **NÃO IMPLEMENTADO**, `EV-R4-004` pendente com cenário/topologia e fontes oficiais datadas. Aprovação de um relay de teste não aprova launcher distribuído.

## AUR-R4-005 — Minecraft, sessões, convites e estados reais

- **Fase/prioridade:** R4/P2; integração após gates aplicáveis.
- **Objetivo:** conectar transporte a um fluxo compreensível do launcher, mantendo capacidades separadas e autorização real.
- **Escopo:** preparar servidor/mundo → criar sessão → autorizar convidado → negociar transporte → validar jogo/serviços → entrar. Servidor público já alcançável conserva conexão normal; servidor privado exige agente/responsável autorizado; dedicado local e mundo aberto para LAN têm preparação própria. Transporte, descoberta de porta, abertura para LAN, auto-join e sync são capacidades independentes. Implementar convites com vínculo criptográfico peer/chave/sessão/permissões, expiração, revogação e anti-replay. Sinalização transporta só metadados mínimos e tem acesso por participante, expiração lógica/retenção e regras testáveis. Autenticação Microsoft/Minecraft e online-mode não são removidos. Sync só por manifesto compatível e consentimento do updater, sem jogo/modpack por Firestore.
- **Dependências:** 001–004 no alcance anunciado; R0/R1 para integração, R1.5/R3 e recuperação relevante para sync. Se contratos de manifesto/convite não estiverem prontos, limitar incremento à integração já possível e manter capacidade indisponível; não marcar gates como concluídos. Há sessões independentes existentes (EST-10) a preservar.
- **Arquivos:** reais a reler: `apps/desktop/src-tauri/src/commands.rs`, `apps/desktop/src-tauri/src/launcher_sessions.rs`, `apps/desktop/src-tauri/src/engine/launch.rs`, `apps/desktop/src-tauri/src/engine/ipc.rs`, `apps/desktop/src/App.tsx`, `firebase/firestore.rules`, `docs/core-ipc.md`. **Propostos:** feature desktop `apps/desktop/src/features/connect/` e comandos/estado por domínio se compatíveis com organização auditada. Hook Core apenas se necessário, com contrato próprio e matriz afetada; nenhuma alteração obrigatória na primeira PoC.
- **Risco:** UI anunciar jogador no mundo após mero socket aberto; convite acessível por terceiro; listener exposto à LAN; mudança local de endereço afetar outras instâncias; retenção sem regra; reintrodução de Companion; sincronização destrutiva sem revisão.
- **Testes planejados:** convites de outro peer/sessão, expirados, revogados e replay; acesso não autorizado a metadados; quota/timeout e retomada; instâncias/launchers paralelos e portas ocupadas; distinguir handshake transporte, status Minecraft, entrada no mundo e evidência de voz; cancelamento em cada etapa; modo público direto preservado. Novas regras de sinalização requerem harness real a criar/verificar, sem inventar comando existente.
- **Critério de aceite:** estados vêm de eventos do backend com evidência definida e mensagens úteis; túnel, jogo e voz separados; status Minecraft não basta para afirmar entrada no mundo; falha de voz não fecha jogo. Convidado não escolhe IP/porta arbitrário. Sincronização somente quando contrato de manifesto/licença/origem/download e rollback tiverem passado; ausência de suporte é mostrada como tal.
- **Rollback:** revogar sessão/convites, interromper transporte e listeners; restaurar somente configurações próprias cuja pós-imagem ainda corresponda ao plano, preservando divergências; não encerrar jogo indiscriminadamente; desabilitar feature sem tocar em dados do usuário.
- **Estado real/evidência:** **NÃO IMPLEMENTADO**, `EV-R4-005` pendente com roteiro nativo/UI e negativas de autorização. Mock de UI não comprova sessão real.

## AUR-R4-006 — Homologar Simple Voice Chat e Plasmo Voice separadamente

- **Fase/prioridade:** R4/P2. Pesquisa iniciada em 001; implementação/homologação após 003/005 no alcance necessário.
- **Objetivo:** encaminhar UDP real e informar corretamente a cada cliente onde sua ponte local está, preservando a segurança de cada mod.
- **Escopo:** escolher um artefato/versionamento de cada mod para uma combinação elegível da matriz preservada, confirmar loader/Minecraft/API/configuração real e registrar hashes/origem/licença. As versões de API/documentação pesquisadas na spec não constituem seleção do JAR. Descobrir escuta do host, anúncio do servidor e destino local do convidado separadamente; investigar `voice_host` e `[host.public]` conforme versão, conflito de porta, múltiplas instâncias e configuração prévia. Preferir API/add-on oficial ou alteração reversível. Não substituir anúncio global por `127.0.0.1` sem pontes existentes em todos os clientes afetados. Para mistura de participantes diretos e Aurora, provar API por participante ou limitar explicitamente o modo. Não reescrever bytes arbitrários do Minecraft criptografado.
- **Dependências:** forwarding UDP autenticado; rede/caminho validado no alcance; mecanismo de mudança com plano/diff/confirmação/snapshot e detecção de divergência. Não usar automaticamente o broker JSON existente para TOML/properties ou senhas/endereço. Hook Core mínimo, caso inevitável, precisa de API real/contrato versionado, explicação da matriz afetada e homologação de regressão; não reintroduzir Assistente in-game.
- **Arquivos:** reais: documentos desta revisão, `docs/engineering/COMPATIBILITY_MATRIX.md`, contratos Core/IPC e padrões de transação citados; fonte específica do Core só após inventário. **Propostos:** adaptadores `engine/connect/adapters/` e fixtures por formato/versão; extensões no Core somente se justificadas. Paths das configurações do mod devem ser obtidos do artefato/instância real, não presumidos.
- **Risco:** encaminhamento funcionando com anúncio errado; configuração global quebrar participantes diretos; nonce/secret/auth do mod comprometido; mod depender de IP original; edição de usuário perdida no restore; voz confundida com TTS desktop.
- **Testes planejados:** cada mod individual, sem exigir ambos ativos; dedicado e LAN somente quando versão suportar; dois ou mais participantes, jogo + voz, autenticação inválida, mod ausente, versão fora da matriz, porta local ocupada e múltiplas instâncias; tamanho real dos datagramas/overhead/MTU, perda/reordenação/saturação e atraso; partida/saída/reconexão; plano cancelado; config alterada por usuário antes de aplicar e antes de restaurar; caminho direto/relay permitido e modo misto somente se anunciado. Não capturar/gravar/transcrever áudio para o forwarding; confirmar funcionamento com mecanismo oficial/observação local autorizada, usando métricas de transporte sem conteúdo.
- **Critério de aceite:** linha específica da matriz possui artefatos, configuração efetiva, evidência do anúncio e porta local por participante, autenticação preservada, retorno correto e teste jogo/voz simultâneo. Criptografia do mod não é reduzida. Limites por IP original registrados; participantes diretos não regressam no modo declarado compatível. Cada mod recebe decisão independente, inclusive “não suportado neste cenário”, sem generalização multiversão.
- **Rollback:** suspender somente o serviço de voz afetado; restaurar pós-imagem correspondente e manter snapshot/journal quando houver divergência; desabilitar adaptador experimental e preservar jogo/configuração editada pelo usuário. Nunca remover mod de terceiro por substring.
- **Estado real/evidência:** **NÃO IMPLEMENTADO**, `EV-R4-006-SVC` e `EV-R4-006-PV` pendentes. Nenhuma combinação de voz foi homologada por esta revisão.

## AUR-R4-007 — Robustez, manifesto, redes reais e liberação gradual

- **Fase/prioridade:** R4/P2, com gates de distribuição R0/R5 e manutenção contínua de R1–R3.
- **Objetivo:** consolidar limites comprovados e liberar apenas capacidades sustentadas por evidência/custo autorizado.
- **Escopo:** executar matriz escolhida, reproduzir falhas, limitar regressões de CPU/RAM/banda e persistência; compatibilidade de modpack por manifesto com origem/licença/hash/download e revisão do updater; documentação de usuário e suporte; coleta local sanitizada/retenção definida; flag experimental por capacidade/mod/caminho. Definir thresholds aceitáveis antes do ensaio a partir de orçamento medido, não inventar ganhos de ping ou porcentagens de sucesso. Produção relay exige nova conferência de termos/gratuidade sustentável; serviço de teste não é promovido por conveniência.
- **Dependências:** incrementos anteriores no conjunto anunciado; evidências R0/R1/R1.5/R2/R2.5/R3/R5 pertinentes preservadas. Falha num serviço não se torna aprovação parcial invisível; capability matrix comunica o limite. Não exige completar todos os nove pares de voz para uma primeira combinação explicitamente restrita, nem amplia os nove pares Core/Skins.
- **Arquivos:** reais: roadmap, spec, backlog, matriz geral, dependências, rastreabilidade, políticas de release e privacidade pertinentes a inventariar. **Propostos:** relatório de homologação por rodada com manifestos/hashes e logs sanitizados, não novos caminhos arbitrários de produção.
- **Risco:** evidência de LAN tratada como CGNAT, RTT chamado latência de áudio, um mod aprovado promover outro, regressão Core/Skins, limite de plano gratuito mudar, sincronização instalar conteúdo proibido, release incompatível com privacidade.
- **Testes planejados:** matriz abaixo e perfis de carga; sessões longas e reconexões; consumo no início/pico/encerramento; alterações de rede, encerramento do host, indisponibilidade/quota de serviços auxiliares; atualizações/rollback e manifesto divergente; logs sem tokens, endereço pessoal ou conteúdo de voz; regressões de Assistente desktop/IPC/Core/Skins quando afetados. Repetições têm motivo/metodologia e tamanho amostral, sem porcentagem fabricada.
- **Critério de aceite:** relatório distingue implementado, teste controlado, teste em redes reais e homologado por combinação; nada sem validação entra habilitado por padrão; budgets rígidos, termos atuais, privacidade e falhas observáveis; rótulos UX alinhados à evidência. Gate R4 exige ameaças/custo/latência medidos e não depender de gasto obrigatório; gate TCP isolado não o fecha.
- **Rollback:** desligar capacidade/provedor/adaptador afetado, encerrar recursos próprios e preservar dados/snapshots; manter versões históricas imutáveis. Nenhum push, instalador, publicação ou deploy é autorizado por este documento de planejamento.
- **Estado real/evidência:** **NÃO IMPLEMENTADO**, `EV-R4-007` pendente. Liberação exige revisão concreta do conjunto final; este backlog não concede autorização operacional.

## Matriz Core/Skins preservada, sem ampliação

Fonte autoritativa: [`scripts/active-mod-matrix.json`](../../scripts/active-mod-matrix.json), schema 1, lida no HEAD indicado. A tabela transcreve seus nove targets; `Core 1.1.5 + Skins 0.3.1` vale para cada linha. Versão de loader de uma rodada runtime deve ser registrada, não confundida com a referência/mínimo do manifesto.

| Minecraft | Loader | loaderVersion | loaderVersionRange | Java runtime | JDK de build |
| --- | --- | --- | --- | ---: | ---: |
| 1.12.2 | forge | 14.23.5.2860 | `[14.23.5,15)` | 8 | 17 |
| 1.16.5 | fabric | 0.13.3 | `>=0.13.3` | 8 | 17 |
| 1.16.5 | forge | 36.2.42 | `[36,)` | 8 | 17 |
| 1.19.2 | fabric | 0.16.14 | `>=0.14.0` | 17 | 17 |
| 1.19.2 | forge | 43.4.16 | `[43,)` | 17 | 17 |
| 1.20.1 | fabric | 0.16.14 | `>=0.14.0` | 17 | 17 |
| 1.20.1 | forge | 47.4.23 | `[47,)` | 17 | 17 |
| 1.21.1 | fabric | 0.16.14 | `>=0.16.10` | 21 | 21 |
| 1.21.1 | forge | 52.1.12 | `[52,)` | 21 | 21 |

Estes nove pares são a matriz de módulos Aurora, não uma declaração de disponibilidade de Simple Voice Chat/Plasmo Voice para todos os targets. Não acrescentar NeoForge, versões de Minecraft ou trilho de servidor diferente implicitamente. O alvo 1.12.2 continua legado isolado e não bloqueia MVP moderno. Build de 18 JARs não é homologação de voz, rede ou runtime.

## Matriz de compatibilidade Aurora Connect proposta

As linhas seguintes são **células de investigação**, não combinações suportadas. “A selecionar” significa que não foi escolhido/baixado/instalado artefato, nem verificada compatibilidade multiversão. Cada linha deverá ser desdobrada para **uma versão exata de mod + Minecraft + loader + Java + configuração + caminho**, nunca aceita com curingas. A seleção de Minecraft/loader deve ser subconjunto da tabela preservada e da disponibilidade oficial real do mod. As referências da spec a API Simple Voice Chat `2.6.20` e documentação/Plasmo Voice `2.1.17`, quando usadas, são fontes de investigação, não preenchimento automático da versão de mod abaixo.

| ID da célula | Mod / versão do JAR | Minecraft / loader | Modo de hospedagem | Caminho de transporte | Endereçamento a comprovar | Estado / evidência |
| --- | --- | --- | --- | --- | --- | --- |
| SVC-D-DIR | Simple Voice Chat / a selecionar | A selecionar entre pares elegíveis existentes | Dedicado com agente autorizado | Direto LAN, IPv4 ou IPv6; registrar um por rodada | Escuta real, `voice_host` efetivo, anúncio recebido, ponte UDP local por convidado | NÃO VERIFICADO / EV-R4-006-SVC pendente |
| SVC-L-DIR | Simple Voice Chat / a selecionar | A selecionar entre pares elegíveis existentes | Mundo aberto para LAN, se suportado pela versão | Direto; registrar um caminho por rodada | Porta dinâmica e ciclo do mundo; anúncio e alocação local distintos | NÃO VERIFICADO / EV-R4-006-SVC pendente |
| SVC-D-REL | Simple Voice Chat / a selecionar | Exatamente o par da rodada aprovada correspondente | Dedicado com agente autorizado | Relay gratuito autorizado para o uso; operador/caminho a fixar | Mesma identidade/autenticação; bridge e máximo de DATAGRAM no relay | NÃO VERIFICADO / dependente também de EV-R4-004 |
| SVC-L-REL | Simple Voice Chat / a selecionar | Exatamente o par da rodada aprovada correspondente | LAN, se suportado | Relay gratuito autorizado; sem promessa de produção | Anúncio da sessão LAN e retorno por participante | NÃO VERIFICADO / dependente também de EV-R4-004 |
| SVC-MIX | Simple Voice Chat / a selecionar | Um par elegível exato | Dedicado e LAN em rodadas separadas, se suportados | Participante direto ao servidor + participante via Aurora | API por participante comprovada ou restrição explícita; anúncio global não basta | NÃO VERIFICADO / não anunciar modo misto |
| PV-D-DIR | Plasmo Voice / a selecionar | A selecionar entre pares elegíveis existentes | Dedicado com agente autorizado | Direto LAN, IPv4 ou IPv6; registrar um por rodada | Escuta real, `[host.public]` efetivo, anúncio recebido, ponte UDP local | NÃO VERIFICADO / EV-R4-006-PV pendente |
| PV-L-DIR | Plasmo Voice / a selecionar | A selecionar entre pares elegíveis existentes | Mundo aberto para LAN, somente se suportado pela versão | Direto; registrar um caminho por rodada | Descoberta de porta/ciclo LAN; suporte não presumido | NÃO VERIFICADO / EV-R4-006-PV pendente |
| PV-D-REL | Plasmo Voice / a selecionar | Exatamente o par da rodada aprovada correspondente | Dedicado com agente autorizado | Relay gratuito autorizado para o uso; operador/caminho a fixar | Autenticação/criptografia e anúncio preservados; DATAGRAM real | NÃO VERIFICADO / dependente também de EV-R4-004 |
| PV-L-REL | Plasmo Voice / a selecionar | Exatamente o par da rodada aprovada correspondente | LAN, somente se suportado | Relay gratuito autorizado; sem promessa de produção | API/configuração LAN efetiva, retorno por participante | NÃO VERIFICADO / dependente também de EV-R4-004 |
| PV-MIX | Plasmo Voice / a selecionar | Um par elegível exato | Dedicado e LAN em rodadas separadas, se suportados | Participante direto ao servidor + participante via Aurora | Adaptação por participante real ou restrição; não substituir globalmente por loopback | NÃO VERIFICADO / não anunciar modo misto |

Servidores públicos já acessíveis com mod de voz configurado usam conexão normal como controle, sem pressupor agente no host. Não há suporte anunciado para servidor privado sem cooperação. Simple Voice Chat e Plasmo Voice são testados em instâncias separadas; não há requisito de coexistência. Cada mod TCP/UDP adicional exige serviço, adaptador e linha de homologação próprios.

## Matriz de cenários e prova exigida

| Grupo | Cenários mínimos | Evidência esperada | Incremento |
| --- | --- | --- | --- |
| TCP | integridade, status/jogo simultâneos, backpressure, EOF/half-close, cancelamento | bytes/hashes de fixture, IDs de conexão, limites de recurso e fechamento | 002 |
| Autorização | chave errada, sessão cruzada, replay, expirado/revogado, destino arbitrário | rejeição antes de IO ao destino; logs sem segredo | 001/002/005 |
| UDP | bidirecional, fronteiras, retorno por jogador/serviço, associações paralelas | eco com identificadores de fixture, origem/associação correta sem payload pessoal | 003 |
| UDP sob falha | perda, reordenação, duplicação, MTU, excesso, saturação, expiração | contadores de descarte/erro, nenhum truncamento, memória limitada, cleanup | 003/006 |
| Conjunto | TCP carregado + UDP de voz; servidor do jogo sob carga | RTT/jitter/perda e uso de recursos separados de desempenho do servidor | 003/006/007 |
| Rede | LAN, IPv4, IPv6 com firewall, NAT duplo, CGNAT observado, redes distintas | topologia sanitizada e caminho observado; LAN/loopback não provam NAT | 004/007 |
| Bloqueios | UDP externo bloqueado, relay indisponível/quota, credencial recusada | caminho alternativo real se suportado, impacto medido ou falha clara | 004/007 |
| Distância | Brasil–Brasil e Brasil–EUA em redes autorizadas | tempos e contexto de rota/localidade geral, sem prometer ganho de ping | 004/007 |
| Configuração | porta ocupada, várias instâncias, plano cancelado, edição concorrente, restauração | diff/hash/snapshot/journal; recusa de restore divergente sem perda | 005/006 |
| Mod por combinação | mod ausente/fora da matriz; dedicado; LAN quando suportado; misto quando anunciado | JAR/hash/loader/Java, config efetiva, anúncio por cliente, jogo e voz reais | 006 |
| Privacidade/custo | logs, permissão de sinalização, expiração, quotas/hard stop | ausência de áudio/segredos; negativas de acesso e condições oficiais datadas | 001/005/007 |

Registrar tempo até transporte autenticado, até serviço TCP acessível, até entrada comprovada no mundo e até validação de voz, com denominadores separados. RTT de rede não mede atraso ponta a ponta do áudio; jitter/perda não medem TPS/MSPT. CPU/RAM/banda precisam de processo/intervalo/carga registrados. Áudio não deve ser gravado nem analisado pelo túnel; observações de experiência ou instrumentos do próprio mod precisam de metodologia explícita. Nenhuma porcentagem de sucesso ou melhoria de ping foi medida nesta revisão.

## Comandos existentes para a futura execução

Os comandos abaixo foram confirmados em `AGENTS.md`, manifests e CI; **não foram executados nesta entrega documental**. Rodar somente os relevantes ao incremento, sem promover compilação a homologação. Se o ambiente não permitir execução, registrar erro/limite real e não inventar aprovação. Comandos de testes novos, de harness Connect ou emulador Firestore ainda não existem e deverão ser criados/verificados antes de serem citados como executáveis.

```powershell
git status --short
git rev-parse HEAD
npm --prefix apps/desktop run build
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
npm --prefix apps/edge-proxy run check
npm --prefix apps/edge-proxy test
npm --prefix functions run lint
npm run core:build
npm run skins:build
npm run release:verify-mods
```

`apps/edge-proxy` usa `wrangler deploy --dry-run` no script `check`; não confundir com implantação. Os três comandos de mods só são pertinentes quando o incremento afetar Core/Skins ou seus contratos/artefatos; builds são sequenciais e requerem ambiente adequado. Não executar `desktop:build`, `release:package`, `deploy`, push ou publicação para provar uma PoC de transporte. A produção e o instalador permanecem sujeitos ao fluxo oficial e a tarefa autorizada própria.

## Resultado desta revisão e próximo passo concreto

Foram produzidos plano e matriz com os requisitos explícitos TCP/UDP/voz/relay gratuito. Não houve alteração de código, rede, conta, billing, configuração de jogador ou infraestrutura. O próximo executor deve atualizar o inventário 001, resolver a escolha principal na ADR e implementar somente o menor incremento seguro verificável: 002 se não existir ponte autenticada; 003 ou o próximo bloqueio real se TCP já estiver implementado e comprovado. UDP/endereçamento de voz permanecem contratos e tarefas concretos; não podem ser removidos como futuro indefinido.
