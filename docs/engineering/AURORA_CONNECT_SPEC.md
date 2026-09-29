# Aurora Connect — especificação candidata de conectividade TCP/UDP

Data da revisão e das consultas externas: **29/09/2026**. Estado do desenho: **HIPÓTESE**. Implementação e homologação desta especificação: **NÃO IMPLEMENTADO** / **PRECISA DE TESTE**. Aurora Connect é nome de trabalho; não é anúncio de recurso disponível. Esta entrega é documental: nenhum transporte, mod, serviço, conta, faturamento ou infraestrutura foi instalado, ativado ou publicado.

O documento detalha R4 do [Master Roadmap](MASTER_ROADMAP.md). A [proposta de ADR](ADR/PROPOSED-aurora-connect-transport.md) compara tecnologias; a [matriz de dependências e zero-spend](AURORA_CONNECT_DEPENDENCIES.md) concentra avaliação de operadores e condições externas. A decisão final e versões fixadas dependem da auditoria e do primeiro incremento. Requisitos explícitos do pedido Aurora Connect V2 prevalecem sobre proibições antigas de qualquer relay.

## 1. Autoridade, leitura e limites da evidência

| Natureza | Conteúdo e consequência |
| --- | --- |
| Original preservado | R4 multiplayer/social, convites autorizados, sincronização por manifesto, PoC e4mc/World Host e adiamento de relay próprio obrigatório; gates R0–R5 continuam vigentes. |
| Requisito novo do responsável | TCP e UDP explícitos; integração separada de Simple Voice Chat e Plasmo Voice; direto prioritário com relay gratuito autorizado possível; zero gasto obrigatório/automático. |
| Evidência documental local lida | `AGENTS.md`, `docs/engineering/MASTER_ROADMAP.md`, `docs/core-ipc.md`, ADR-018, `COMPATIBILITY_MATRIX.md` e `scripts/active-mod-matrix.json`. O arquivo canônico encontrado é `MASTER_ROADMAP.md`; este documento não afirma ter lido um anexo chamado `MASTER_ROADMAP(1).md`. |
| Evidência de manifests lidos | HEAD desta base: `dc2ec5ac36a88f54b1130bac4789f8ecd56641d2`; desktop/núcleo `0.1.1-alpha.13`, React 19/TypeScript, Tauri 2 e Rust confirmados por `apps/desktop/package.json` e `apps/desktop/src-tauri/Cargo.toml`. Core 1.1.5 e Skins 0.3.1 confirmados pela matriz ativa. |
| Evidência externa | Fontes oficiais consultadas na seção 15. Documentação de fornecedor prova contrato publicado, não funcionamento no Aurora; páginas sem versão fixada precisam ser confrontadas com o artefato escolhido. |
| Proposta técnica | Todo contrato, limite, estado, fluxo, adaptador, teste e nome de mensagem introduzido abaixo é candidato, salvo identificação explícita de contrato existente. Não há aprovação de protocolo final nem homologação. |
| Limitação | Não foram executados Minecraft, mods de voz, testes de NAT, sessões remotas ou benchmark. A leitura delimitada acima não é auditoria completa de código, segurança, licenças ou termos. Não se inferem funcionalidades existentes por nomes de arquivos ou dependências transitivas. |

O IPC existente tem protocolo **1**, WebSocket exclusivamente em `127.0.0.1`, porta efêmera, nonce por execução, handshake e sessão pública sem credenciais. O contrato Connect candidato tem namespace e versão próprios; não altera silenciosamente esse IPC. Conforme ADR-018, nenhum adaptador de rede recupera Companion, HUD, Assistente no jogo, gravação ou análise de voz. O Assistente desktop e a homologação dos nove pares Core/Skins permanecem independentes. A matriz ativa é a fonte dos pares; esta especificação não a amplia para NeoForge ou qualquer outra versão.

## 2. Objetivo e fronteiras de produto

Compartilhar um servidor ou mundo com convidados autorizados, reduzindo configuração manual, incluindo jogo e serviços TCP/UDP explicitamente homologados. Minecraft continua autoritativo; P2P descreve a conectividade entre agentes. O host, o jogo e os serviços precisam permanecer disponíveis. Hospedagem oficial 24 horas não integra R4.

| Modo | Capacidade e pré-condição | Limite explícito |
| --- | --- | --- |
| Servidor público acessível | Cliente usa o endereço público normal; launcher pode validar manifesto/estado, se autorizado. | Não precisa de túnel para o jogo. Voz tem endpoint e validação próprios; TCP acessível não prova UDP. |
| Servidor privado cooperativo | Responsável autoriza agente Aurora no host e registra destinos locais exatos. | Convite não dá acesso a servidor privado de terceiros sem cooperação. |
| Dedicado local | Agente encontra ou recebe endpoint efetivo e confirma autorização antes de expor serviço. | Iniciar servidor, aceitar EULA e alterar configuração são operações distintas; não decorrem de abrir túnel. |
| Singleplayer aberto para LAN | Mundo já aberto e porta efetiva descoberta, manualmente na PoC ou por adaptador comprovado depois. | Abrir LAN, descobrir porta e confirmar servidor integrado dependem de versão/loader; não são fornecidos automaticamente pelo transporte. |

As capacidades devem ter estados separados: transportar bytes; descobrir porta; abrir mundo para LAN com autorização; configurar anúncio dos mods; entrar automaticamente; conferir/sincronizar modpack. A primeira ponte pode receber IP/porta/chave explicitamente em ambiente de desenvolvimento. Esse roteiro não comprova a experiência final de um clique.

O fluxo normal suportado não exige VPN externa instalada manualmente, driver, administrador permanente ou abertura manual de portas. Se uma rede exigir ação não homologada, mostrar limite; não contornar silenciosamente firewall, política da organização ou cobrança. Configurar exceções de sistema, mapear roteador e persistir alterações exige consentimento específico e recuperação.

## 3. Camadas candidatas e fluxo

| Fronteira | Responsabilidade | Fora de sua autoridade |
| --- | --- | --- |
| Identidade e autorização | Vincular participante, chave do transporte, sessão, convite e serviços permitidos; expirar/revogar. | Reutilizar token Microsoft/Minecraft como credencial P2P ou desativar autenticação do jogo. |
| Sinalização | Entregar metadados mínimos autenticados aos destinatários corretos. | Carregar jogo, áudio ou arquivos de modpack; conceder serviço com base em manifesto. |
| Descoberta e travessia NAT | Resolver peer/endereço e observar caminhos da biblioteca escolhida. | Prometer NAT universal; tratar STUN como relay ou introduzir ICE redundante. |
| Transporte | Criptografia ponta a ponta, streams confiáveis, datagramas, limites e telemetria real. | Decidir portas autorizadas, interpretar áudio ou reimplementar criptografia/congestionamento. |
| Registro/ponte de serviços | Converter serviço autorizado em TCP/UDP local e garantir retorno correto. | Proxy aberto, SOCKS genérico ou acesso geral à LAN. |
| Adaptador Minecraft/mod | Descoberta e endereçamento por contrato comprovado da versão, sem mudar payload opaco no túnel. | Criar voz própria, capturar áudio, reinstalar Companion. |
| Interface | Preparação, consentimento, estado backend e diagnóstico redigido. | Simular sucesso por tempo decorrido ou tomar manifesto como autorização. |

Implementar módulos pequenos no núcleo existente, quando adequado, sem um monólito Connect e sem inventar um framework de plugins geral. Os caminhos reais relevantes são `apps/desktop/src-tauri/src/engine`, `apps/desktop/src`, `apps/aurora-core` e `docs/core-ipc.md`. Novos caminhos e símbolos de implementação serão propostos após inventário; esta especificação não afirma que existem.

```text
TCP: Minecraft/mod do convidado
       → listener TCP loopback exclusivo da instância
       → stream bidirecional confiável autenticado
       → agente do host → endpoint TCP autorizado

UDP: mod do convidado
       → listener UDP loopback exclusivo da instância/serviço
       → datagrama autenticado com associação identificada
       → agente do host → socket da associação → endpoint UDP autorizado

Retorno UDP: endpoint autorizado → socket da associação no host
       → mesmo peer/sessão/serviço/associação → listener do convidado
       → endereço local de origem do mod correspondente
```

Sequência candidata de sessão:

1. Host escolhe instância/servidor/mundo e visualiza serviços, destinos e limitações; agente inspeciona somente recursos autorizados. Se necessário, gera plano/diff de configuração e snapshot.
2. Host cria sessão e autoriza cada convidado. Convite vincula host/chave/sessão/permissões/validade; material secreto não vai a logs.
3. Convidado confirma identidade do host, escolhe instância compatível e reserva listeners loopback. Configuração/endereço são resolvidos por instância, não globalmente.
4. Agentes negociam transporte e validam autorização mútua antes de abrir destinos; anunciam capacidades efetivas separadas para streams/datagramas e caminho direto/relay permitido.
5. Registro fornece apenas serviços permitidos. Ponte responde abertura/erro. Minecraft e cada mod recebem endereços corretos por mecanismo homologado, com confirmação de aplicação.
6. UI valida transporte, servidor, entrada no mundo e mod separadamente. Auto-join, quando comprovado para a combinação, usa a ponte TCP; usuário pode continuar no jogo com falha de voz.
7. Encerrar/revogar fecha streams, associações e listeners, retira mapeamentos com lease e restaura apenas alterações próprias sem conflito. Host ausente encerra a sessão; não há migração de autoridade do mundo implícita.

## 4. Contrato pequeno de serviços, proposto

Os nomes abaixo são **HIPÓTESE**. Antes de codificar, comparar com modelos de sessão/instância/manifests e envelopes existentes; reutilizar identidades quando seguras. Não é versão nova do IPC Core nem manifesto v3 pronto.

### 4.1 Registro privado e projeção para convidados

| Campo candidato | Escopo | Regra |
| --- | --- | --- |
| `schemaVersion: 1` | registro/controle | Versão exata negociada; incompatível falha explicitamente. |
| `sessionId`, `sessionEpoch` | sessão autenticada | ID opaco e geração; recriar sessão invalida associações antigas. |
| `serviceId` | único na sessão | ASCII limitado e estável na sessão; `minecraft-game`, `simple-voice-chat`, `plasmo-voice` são exemplos propostos. |
| `protocol` | registro | Enum `tcp` ou `udp`; mesmo número de porta não funde serviços. |
| `hostEndpoint` | **somente host** | Endereço literal/porta efetiva, fixados após aprovação local. PoC restringe a loopback. Destino LAN diferente exige extensão explícita e autorização, sem varredura. |
| `permissions` | host e projeção filtrada | Participantes/scopes autorizados pelo host, validade e limite; deny por padrão. |
| `limits` | política efetiva negociada | Conexões/associações, bytes, frequência, filas e payload máximo; convidado nunca aumenta teto do host. |
| `requiredCapabilities` | projeção | Por exemplo, stream bidirecional ou datagrama bidirecional; ausência pode bloquear somente o serviço. |
| `adapter` | metadados verificados | ID/versão do adaptador, mod, Minecraft/loader e modo de hospedagem testados. Manifesto sugere, não autoriza. |
| `configurationRevision` | privado no host | Hash/revisão do destino/config aplicada para detectar divergência, sem conteúdo sensível. |

Convidado recebe uma projeção mínima com identificador, protocolo, limites e disponibilidade. Não recebe autoridade para enviar IP/porta arbitrário. A abertura candidata `OpenService(serviceId, connectionId)` resolve o destino no host somente depois de autenticação e validação de sessão. `OpenService` é nome proposto, não API existente. Atualizar destino exige nova aprovação/revisão e invalida aberturas incompatíveis; DNS mutável e metadados remotos não podem redirecionar o host.

A sessão só admite serviços de jogo e adaptadores explicitamente autorizados. Recusar RCON, SSH, bancos de dados, painéis administrativos, multicast/broadcast e redirecionamento genérico. Porta baixa ou número conhecido isoladamente não é detector suficiente: categoria, processo e destino aprovados devem compor a política. Não expor shell nem execução remota.

### 4.2 Controle e datagramas

Controle usa canal confiável separado de bytes TCP: negociar versão/capacidades, abrir/aceitar/recusar/fechar serviço, criar/remover associação, revogar sessão. Cada mensagem tem limite e prazo, erros enumerados e request ID sem credencial. Controle não carrega áudio; prioridade e limites evitam starvation por jogo.

Para UDP, a tabela autenticada relaciona `(peerKey, sessionId, sessionEpoch, serviceId, associationId)` a destino aprovado e origem local do mod. Um cabeçalho binário compacto candidato contém versão/tipo, índice de serviço, ID de associação, geração e tamanho do payload. A forma exata deve ser fixada e testada antes da primeira ponte UDP. **Reservar até 32 bytes** para o enquadramento Aurora na PoC é teto de projeto, não alegação sobre overhead real da biblioteca. IDs nunca autorizam por si; só são válidos na conexão e tabela autenticadas. Não mandar endereço arbitrário por datagrama. Rejeitar tamanho divergente, índice desconhecido, geração velha e versão incompatível antes de enviar UDP ao host.

Autenticar a criação da associação pelo controle; um pacote UDP desconhecido não aloca recursos sem limites. A confirmação precisa anteceder dados úteis ou o remetente aceita perda até confirmação. Não reutilizar associação/generation encerrada; descartar respostas tardias e pacotes de uma sessão anterior.

## 5. Ponte TCP

A unidade é uma **conexão de bytes da aplicação**, não pacote IP/TCP. Cada conexão loopback aceita ganha um stream bidirecional e uma conexão TCP host separada. Consulta de status, login, jogo e conexões concorrentes não compartilham um stream por engano. O transporte confiável conserva ordem/integridade em cada direção; nunca mandar bytes TCP como UDP sem confiabilidade.

Requisitos candidatos verificáveis:

- Validar peer/sessão/serviço antes de conectar ao endpoint host e antes de qualquer byte da aplicação. Limitar espera de abertura e recusar excedentes.
- Bombear ambas as direções independentemente. Leituras param quando buffer cheio; escritas parciais são concluídas corretamente. Não acumular mundo/chunks em fila ilimitada.
- Propagar EOF como fechamento da direção correspondente; permitir que a outra direção termine. Erro/reset/cancelamento encerra o par e libera recursos, com semântica documentada do transporte escolhido.
- Encerrar instância, sessão ou convite revogado cancela tarefas dependentes. Fechar uma consulta de status não encerra jogo nem voz de outro convidado.
- Não alterar login, compressão, criptografia, payload de plugin ou mensagens dentro do stream. Uma substituição de bytes não consegue reescrever anúncio em fluxo Minecraft criptografado de forma válida.
- Não reduzir `online-mode`, whitelist, permissões ou autenticação oficial. IP observado pelo servidor pode virar loopback/agente host; preservar essa limitação em diagnósticos e avaliar plugins com restrição por IP. Não inventar cabeçalho de identidade que o servidor não entende.

Um fluxo ocioso de jogo pode ser legítimo: não impor timeout curto que derrube jogador parado. Prazo de abertura, progresso sob buffer cheio, heartbeat do transporte e cancelamento são políticas distintas.

## 6. Ponte UDP e retorno correto

### 6.1 Associação e semântica

O listener local do convidado fica em `127.0.0.1` e, somente se testado, em `::1` com política IPv6 explícita. Uma instância/serviço recebe porta própria reservada por bind; não selecionar porta por sondagem e liberá-la antes de usar. A origem local efetiva do mod compõe a associação. Processo loopback alheio não deve adquirir acesso geral por escolher uma porta: exigir contexto de sessão/instância, autorização do serviço e limites; loopback não é autenticação de usuário local.

**Candidato inicial: um socket UDP conectado ao endpoint autorizado por associação no host.** Sua porta de origem distinta permite que respostas do mod servidor retornem ao socket correto. A tarefa de leitura desse socket conhece peer/sessão/serviço/associação, reencapsula a resposta e envia apenas ao convidado associado. No convidado, o mesmo listener envia o payload integral ao endereço local original do mod. O socket dedicado restringe remetente; qualquer API que permita receber outra origem precisa conferir IP/porta exatos. Não fundir sockets antes de demonstrar demultiplexação correta, inclusive respostas simultâneas, reuso de porta e pacotes tardios.

A ponte conserva uma mensagem UDP por datagrama lógico, inclusive conteúdo binário e comprimentos distintos. Não concatena mensagens, não transforma fronteiras em stream e não retransmite indiscriminadamente áudio. Perda, duplicação e reordenação do caminho permanecem características observáveis; não se promete exatamente uma vez. O mod continua responsável por autenticação e criptografia do seu protocolo. Associações expiram por ociosidade e fecham por revogação/shutdown; keep-alive legítimo do mod renova atividade.

Serviços que dependam do IP original, resposta por porta diferente, broadcast ou tráfego iniciado pelo servidor antes da associação exigem contrato próprio. O candidato unicast de destino fixo não os suporta automaticamente. Uma associação não pode escolher novo destino por pacote nem servir como amplificador público.

### 6.2 DATAGRAM, tamanho e MTU

QUIC DATAGRAM é candidato apropriado para preservar mensagens sem retransmissão de stream; exige suporte negociado nos dois sentidos. A RFC 9221 distingue limite recebido de frame e tamanho viável no caminho, não fornece fragmentação de DATAGRAM e exige que a aplicação multiplexe os fluxos. ACK de transporte não prova consumo pelo mod. Fonte: [RFC 9221, §§3–5](https://www.rfc-editor.org/rfc/rfc9221.html), consulta 29/09/2026.

O teto efetivo de payload do serviço será:

`min(limite do serviço, limite de payload DATAGRAM retornado pela biblioteca no caminho atual − cabeçalho Aurora)`.

Antes de implementar, confirmar se a API retorna tamanho de frame ou payload; não subtrair overhead QUIC duas vezes. A biblioteca deve calcular sua parcela QUIC/criptografia/caminho; a aplicação contabiliza seu enquadramento e qualquer encapsulamento que a API não inclua. Em mudança direto↔relay, atualização de MTU ou conexão renegociada, reavaliar teto antes de anunciar disponibilidade do serviço.

- Não assumir que 1.200 bytes de capacidade de pacote QUIC equivalem a 1.200 bytes úteis do mod, nem que o limite teórico UDP cabe no transporte.
- Na PoC genérica, começar com payload de serviço candidato de **1.024 bytes**, sempre reduzido ao teto efetivo. Isso **não** declara voz compatível. Medir histograma de tamanhos de datagramas opacos por versão/config, sem coletar conteúdo.
- Recepção local usa buffer capaz de detectar datagrama excedente sem truncá-lo (por exemplo buffer máximo de UDP e checagem de tamanho, ou API de truncamento). Todo pacote acima do teto é rejeitado inteiro, gera contador/erro agregado e indisponibilidade explícita quando impedir o serviço; jamais entregar prefixo silenciosamente.
- Não fragmentar na primeira ponte. Se tamanhos reais exigirem fragmentação, propor incremento separado: ID/generation autenticados, total/tamanho limitados, quota por peer, timeout curto de remontagem, descarte integral incompleto, proteção a duplicados/overlap e testes negativos. Antes disso, anunciar a restrição de compatibilidade.
- Reduzir um parâmetro do mod não é correção automática: exige suporte documentado, plano/diff, autorização, snapshot e homologação de qualidade/funcionamento. Caso contrário, preservar configuração e bloquear somente o serviço incompatível.

O `mtu_size` publicado pelo Simple Voice Chat tem referência de **1.275 bytes** na página consultada; o significado exato no artefato precisa ser conferido e os tamanhos realmente emitidos medidos. Não há evidência de que caiba no teto candidato da ponte. Esta diferença é um risco de projeto a resolver antes da homologação do mod, não motivo para afirmar suporte parcial a áudio truncado. Fonte: [configuração SVC](https://modrepo.de/minecraft/voicechat/wiki/server_config).

### 6.3 Filas, concorrência e congestionamento

Áudio usa fila curta por associação, com expiração por tempo de permanência local e descarte do vencido; não exige inspecionar nem decodificar áudio. Não aplicar a mesma regra de descarte a outros protocolos UDP: o perfil do adaptador define preservação, erro ou descarte permitido. Sob saturação, o jogo não pode consumir memória ilimitada nem o controle desaparecer; medir justiça entre peers.

Compartilhar conexão autenticada para streams e datagramas é candidato, condicionado a prioridades reais da biblioteca, contenção do controlador de congestionamento e testes TCP+UDP simultâneos. Separar filas sempre; separar conexões quando as medições justificarem, vinculando ambas à mesma autorização. Não impor único socket ou conexão. Não criar controlador de congestionamento próprio nem criptografia paralela.

### 6.4 Limites iniciais de laboratório, calibráveis

Estes números são **HIPÓTESE**, não SLA, capacidade final, configuração dos mods ou autorização de quota externa. Servem para tornar abuso, memória e limpeza testáveis. Antes da liberação, medir e revisar por versão/plataforma, respeitando sempre o menor limite entre host, serviço e provedor.

| Recurso | Candidato para PoC | Resposta ao exceder |
| --- | --- | --- |
| Sessões/participantes | 1 sessão hospedada; até 8 convidados autenticados | Recusar nova admissão sem derrubar existentes. |
| Serviços | Até 8 serviços aprovados por sessão | Recusar registro; nenhum registro automático por mod/manifesto. |
| Streams TCP | 8 por convidado, 64 no host | Recusar abertura com erro de capacidade. |
| Buffers TCP da ponte | 64 KiB por direção/conexão; teto agregado da ponte 16 MiB | Backpressure e cancelamento explícito em erro; contabilizar buffers da biblioteca separadamente. |
| Associações UDP | 4 por convidado/serviço, 16 por convidado, 128 por host | Recusar criação sem alocação adicional. |
| Fila UDP genérica | Até 32 datagramas e 64 KiB por associação; teto agregado 8 MiB | Política declarada por serviço; contadores de descarte, sem truncamento. |
| Fila de voz candidata | Até 16 datagramas e 40 ms de permanência | Descartar vencidos; confirmar com mod e carga real. |
| Taxa UDP | 500 datagramas/s e 512 KiB/s por associação, burst 2× por até 1 s | Limitar ambas as direções; calibrar fan-out de voz no host, sem confundir com taxa real do codec. |
| Banda total da ponte | 8 MiB/s agregado no host | Backpressure TCP/descarte UDP conforme perfil; contadores locais. |
| Associação ociosa | Expirar após 60 s sem tráfego válido | Fechar socket/tabela; recriação exige controle autenticado. |
| Abertura/controle | 10 s por abertura; 16 KiB por mensagem; fila 64; 10 mensagens/s por peer, burst 20 | Cancelar/recusar ou desconectar abusador. |
| Convite | Validade inicial de 10 min para ingresso, uso único por destinatário | Expirado/consumido é negado. Sessão ativa exige autorização própria ainda válida. |
| Sessão e encerramento | Lease candidato de 2 h renovável explicitamente; shutdown alvo ≤5 s | Expiração/revogação fecha dados e limpa recursos; medir, sem garantia antes do teste. |

Teto de memória total do processo, buffers QUIC, tarefas, handles e sockets ainda precisa ser medido; os tetos de filas acima não provam limite de RSS. Impor limites da biblioteca e admissão antes de alocar. Pacote não autenticado não renova lease, gera resposta volumosa nem provoca conexão a destino.

## 7. Endereçamento e compatibilidade dos mods

Integrar mods existentes faz parte explícita de R4. Não haverá voice chat Aurora próprio. Simple Voice Chat e Plasmo Voice têm linhas de homologação independentes; não se presume instalação simultânea.

### 7.1 Três endereços distintos

| Endereço | Quem controla | Exemplo conceitual, sem porta fixa |
| --- | --- | --- |
| Escuta do serviço no host | Config/API real do mod e responsável pelo host | Mod servidor vinculado a endereço local e porta efetiva. |
| Endereço anunciado ao cliente | Servidor/mod/API por participante, se disponível | Nome público para jogador direto; alvo da ponte local para jogador Aurora quando validado. |
| Ponte local de cada convidado | Aurora da instância convidada | `127.0.0.1:<porta-reservada>`; não existe automaticamente em outro PC. |

Portas efetivas vêm de configuração validada ou API/evento confirmado, com correlação à instância. Não deduzir porta só pelo número da porta Minecraft nem escanear LAN. Configuração persistida pode diferir do runtime após reload/falha; preferir evidência runtime quando disponível. TCP e UDP de mesmo número são dois serviços. Duas instâncias, janelas ou contas podem precisar de portas diferentes; testar reabertura, IPv4/IPv6 loopback e porta ocupada antes de anunciar endereço.

**Endereço global `127.0.0.1` não é solução universal.** Só aponta para o computador de cada cliente. Um participante direto sem ponte falha; portas locais diferentes também falham. Não alterar globalmente anúncio para acomodar um convidado sem avaliar todos os participantes. Se API por participante não for comprovada, limitar explicitamente o cenário experimental a conjunto controlado com mapeamento uniforme/reservável, ou manter o recurso indisponível naquele modo. Modo misto não pode ser anunciado como suportado antes de prova.

### 7.2 Simple Voice Chat

**Evidência externa:** documentação consultada distingue tráfego UDP, `port`, `bind_address`, `voice_host` e configurações de servidor/proxy. `voice_host` anuncia endereço aos clientes e pode conter porta. A página marca `port`, `bind_address` e `voice_host` como aplicáveis a servidor dedicado; logo não comprovam configuração de mundo LAN. `config/voicechat/voicechat-server.properties` é caminho publicado para mods, não arquivo comprovadamente presente na instância Aurora. Fonte: [Server Config File](https://modrepo.de/minecraft/voicechat/wiki/server_config).

O guia oficial de proxy UDP orienta anúncio do proxy via `voice_host`. Plugins oficiais de Velocity/Bungee/Waterfall têm comportamento próprio e podem ignorar o `voice_host` do servidor; não equivalem a proxy UDP genérico. Nenhum desses guias prova que todos os clientes Aurora têm ponte na mesma porta. Fonte: [Proxy Setup](https://modrepo.de/minecraft/voicechat/wiki/proxy_setup).

**API real consultada:** o índice publicado identifica versão API **2.6.20**, que não fixa versão/hash dos JARs da futura PoC. `VoiceHostEvent.getPlayer()` identifica o jogador e `setVoiceHost(String)` altera o host enviado ao cliente: é candidato concreto a anúncio por participante, condicionado a artefato/loader e correlação autenticada jogador↔peer. `ClientVoicechatInitializationEvent.setSocketImplementation(ClientVoicechatSocket)` permite implementação de socket cliente; a interface inclui abertura, leitura, envio e fechamento. Pode ser alternativa de adaptação local, mas exige manter semântica de retorno e fechamento, sem decodificar áudio nem exportar segredos. Fontes: [índice API](https://voicechat.modrepo.de/), [VoiceHostEvent](https://voicechat.modrepo.de/de/maxhenkel/voicechat/api/events/VoiceHostEvent.html), [evento cliente](https://voicechat.modrepo.de/de/maxhenkel/voicechat/api/events/ClientVoicechatInitializationEvent.html), [socket cliente](https://voicechat.modrepo.de/de/maxhenkel/voicechat/api/ClientVoicechatSocket.html).

**Proposta a validar:** preferir configuração reversível em dedicado quando atender todos os participantes; para cenário misto, estudar add-on oficial usando `VoiceHostEvent`, preservando anúncio dos participantes diretos. Reservar a ponte local antes do anúncio por jogador. Alternativa cliente pode redirecionar somente a instância autorizada e deixar servidor/anúncio global intactos; precisa provar endereço de retorno esperado pelo mod. Não somar essas alternativas como implementações obrigatórias.

**Estado de versões/loaders:** a página oficial lista versões Minecraft/loaders; a interseção exata com cada target Aurora deve ser conferida por artefato, sem presumir cobertura dos nove pares. Versão de mod por alvo, hash, Java, configuração efetiva, disponibilidade da API em cada build e licença de distribuição permanecem **NÃO VERIFICADO** até seleção/inventário. Não ampliar a matriz por a documentação também mencionar NeoForge/Quilt. Fonte: [Supported Versions](https://modrepo.de/minecraft/voicechat/wiki/supported_versions). `mtu_size`, LAN, anúncio por jogador, autenticação, perda de IP original, reinício e múltiplos convidados precisam de provas separadas.

Para evidenciar conexão, `ClientVoicechatConnectionEvent.isConnected()` é um sinal publicado da conexão do mod; não prova microfone, audibilidade nem qualidade. Fonte: [evento de conexão](https://voicechat.modrepo.de/de/maxhenkel/voicechat/api/events/ClientVoicechatConnectionEvent.html). Não usar eventos de áudio cru para realizar encaminhamento ou telemetria.

### 7.3 Plasmo Voice

**Evidência externa:** `[host]` relaciona escuta e endereço do serviço; `[host.public]` permite separar endereço anunciado. A interpretação documentada de porta zero e endereço inferido precisa ser conferida no artefato/config efetivos. Não transportar automaticamente UDP só porque TCP funciona. Fonte: [Advanced Theory](https://plasmovoice.com/docs/server/advanced/), [instalação de servidor](https://plasmovoice.com/docs/server/installing/).

**Referência versionada consultada:** release oficial **2.1.17**, publicada em 15/09/2026, lista assets Fabric para 1.16.5, 1.19.2, 1.20.1 e 1.21.1. Esta constatação não seleciona nem instala esses JARs; correspondência de Forge, Java, hashes e cada modo continua a verificar. A documentação genérica anuncia singleplayer/Open to LAN, mas não comprova o endereço efetivo nem integração Aurora. Fontes: [release oficial](https://github.com/plasmoapp/plasmo-voice/releases/tag/2.1.17), [produto](https://plasmovoice.com/).

O Dokka padrão abriu em `2.2.0-SNAPSHOT`; foi escolhida para pesquisa a documentação versionada [2.1.17](https://dokka.plasmovoice.com/older/2.1.17/index.html). A API possui módulos de cliente, servidor e proxy e mecanismo de add-ons; isso é extensão real, não garantia de qualquer alteração desejada. Fonte: [Getting Started](https://plasmovoice.com/docs/api/).

**APIs reais investigadas:** `su.plo.voice.api.client.event.socket.UdpClientConnectEvent` documenta a oportunidade de substituir o cliente UDP antes da conexão; a página renderizada não detalha todos os mutadores necessários, portanto não se inventa um `setAddress` ou assinatura de implementação. O futuro incremento deve ler fonte e artefato **2.1.17 escolhido** para completar o contrato. `TcpPacketSendEvent` existe no servidor, mas existência de evento não prova substituição segura e individual de anúncio. Fontes: [UdpClientConnectEvent 2.1.17](https://dokka.plasmovoice.com/older/2.1.17/client/su.plo.voice.api.client.event.socket/-udp-client-connect-event/index.html), [TcpPacketSendEvent 2.1.17](https://dokka.plasmovoice.com/older/2.1.17/server/su.plo.voice.api.server.event.connection/-tcp-packet-send-event/index.html).

**Proposta a validar:** configuração reversível de `[host.public]` somente em topologia em que todos os anúncios continuem corretos. Para participantes diretos e Aurora, estudar add-on cliente oficial que adapte o endpoint por instância, ou mecanismo servidor por participante comprovado na fonte. Na ausência dessa prova, registrar cenário misto como **NÃO VERIFICADO**, sem alteração global para loopback. Transporte UDP sozinho não fecha esta tarefa.

Eventos `UdpClientConnectedEvent`, `UdpClientClosedEvent` e `UdpClientTimedOutEvent` existem no pacote cliente; avaliar semântica e correlação de instância antes de usá-los como evidência UX. Fonte: [eventos de socket 2.1.17](https://dokka.plasmovoice.com/older/2.1.17/client/su.plo.voice.api.client.event.socket/index.html). Não usar hooks de áudio/criptografia para fazer a ponte, não reduzir autenticação e não enviar segredos do mod à sinalização.

### 7.4 Core opcional e alterações reversíveis

Um add-on oficial específico tem preferência sobre hooks frágeis internos. Se for inevitável um hook mínimo no Aurora Core, propor ADR/contrato restrito de descoberta/endereço/estado por instância, com nonce e capabilities pelo IPC existente. O hook não recebe JWT, chave do provedor ou tokens Minecraft/Microsoft; no máximo dados públicos e capacidade local efêmera estritamente necessária. O nonce IPC existente não se torna segredo P2P nem autorização de host remoto.

Antes de implementar hook, registrar matriz afetada lendo `scripts/active-mod-matrix.json`, diferenças dos adaptadores Java e regressões de IPC/Core/Skins. Homologação de voz em um par não homologa os nove; incompatibilidade de voz num par não autoriza remover o par do launcher. Não impor Core ou qualquer hook à PoC genérica TCP/UDP. Não criar Companion ou novo assistente/HUD.

Toda mutação de configuração segue: detectar versão/arquivo efetivo → plano com antes/depois e instâncias afetadas → autorização apropriada → bloqueio de mutação da instância e snapshot → escrita atômica/journal → leitura de confirmação → validar runtime. Conservar comentários/campos alheios quando o formato permitir; não reconstruir arquivo inteiro sem necessidade. No rollback, comparar hash/revisão aplicada: se houve edição concorrente, restaurar apenas campos próprios comprovadamente inalterados ou apresentar conflito/diff, preservando ambas as versões. Nunca sobrescrever silenciosamente edição posterior. Crash/reinício devem recuperar journal sem reabrir serviço automaticamente.

Licença, origem, SHA-256, loader e autorização de download dos mods pertencem ao updater e aos gates R0/R1.5. CurseForge sem URL autorizada mantém ação oficial manual. Não redistribuir JAR por túnel/Firestore nem criar instalação paralela que contorne inventário/snapshot.

Licenças observadas em fonte oficial em 29/09/2026: o [arquivo `license` do SVC, branch 1.20.1](https://github.com/henkelmax/simple-voice-chat/blob/1.20.1/license), declara direitos reservados; não inferir licença permissiva de um template de API ou da publicação no GitHub. O [LICENSE do Plasmo na tag 2.1.17](https://github.com/plasmoapp/plasmo-voice/blob/2.1.17/LICENSE) contém LGPLv3. Esses registros não substituem seleção de JAR, licença específica da API, obrigações de componentes empacotados e regras da origem de download; autorização de redistribuição pelo Aurora continua **NÃO VERIFICADO**.

## 8. Identidade, convite, segurança e privacidade

### 8.1 Modelo de ameaça

Adversários considerados: convidado sem permissão; convite vazado, expirado, revogado ou repetido; peer com chave trocada; servidor de sinalização/relay malicioso; mod/manifesto não confiável; processo local tentando usar listener; participante autenticado abusivo; pacote grande/fragmentação incompleta; destino host alterado; encerramento parcial; provedor indisponível. Host e convidado ainda confiam nos próprios sistemas operacionais e mods instalados. Não prometer isolamento contra malware local nem anonimato de IP.

| Ameaça | Contrato de mitigação e teste |
| --- | --- |
| Peer errado/MITM | Validar chave esperada com identidade do convite/sessão; criptografia ponta a ponta entre agentes, inclusive relay. Não aceitar qualquer certificado. Negar chave/certificado divergente antes de abrir serviço. |
| Replay | Convite assinado/autenticado, destinatário, sessão/generation, expiração, nonce aleatório e registro de consumo; desafio de prova da chave. Negar repetição noutra sessão/peer. Não usar 0-RTT para ingresso/abertura com efeito até perfil antirreplay validado. |
| Revogação | Host é autoridade da sessão e aplica revogação local imediatamente a novos/atuais fluxos; cancela associações e streams. Revalidação após reconexão; cache de convite não renova permissão. |
| Proxy aberto/SSRF | Somente service ID aprovado, endereço host fixado e autorização local; negar IP/porta enviados pelo convidado, DNS redirecionado e catálogo adulterado. |
| Amplificação/DoS | Sem encaminhamento antes de autenticação; limites de admissão, datagramas, streams, memória, taxa e destino de retorno; mensagens de erro curtas e agregadas. |
| Confusão entre instâncias/jogadores | Correlacionar instância local, identidade de peer e sessão. UUID/username vindos do convidado não provam identidade Minecraft. Mapeamento por jogador para add-on exige vínculo verificável com sessão Minecraft autenticada. |
| Vazamento | Chaves privadas no armazenamento apropriado; logs sem tokens/convites/endpoints completos; sinalização mínima; payload opaco em memória, sem gravação. |

A autorização do transporte não substitui autenticação Minecraft/mod. Preservar `online-mode` e modos oficiais existentes; não recomendar offline-mode para tornar ponte funcional. Mod de voz continua verificando sua identidade/segredo e criptografia. Não enviar tokens Microsoft/Minecraft/Firebase de usuário para peers ou relays; o mecanismo de sinalização usa só credencial própria estritamente necessária. Não embutir segredo compartilhado global ou chave administrativa de provedor no binário.

Uma conta Aurora pode autorizar o convite, mas precisa de associação verificável com a chave do agente; um UUID informado pelo cliente não basta. Para PoC, troca explícita de chaves esperadas por canal autorizado e fingerprint conferido pode servir; é configuração de desenvolvimento, não identidade de produto pronta. Recuperação/rotação de chave invalida convites anteriores até autorização renovada.

### 8.2 Sinalização e retenção

Reutilizar Firebase/Firestore ou Worker somente depois de ler regras, contrato e quotas reais. Não abrir coleção permissiva para acelerar PoC. Candidato: metadados por sessão e participante, host owner, leitura restrita aos envolvidos, escrita limitada ao próprio envelope e transições de estado validadas. Campos mínimos: IDs opacos, chaves públicas/fingerprints, versão, capacidades, convite autenticado com expiração e endereços candidatos estritamente necessários. Endereços e presença são dados pessoais/contextuais: exposição limitada, sem indexação pública.

Expiração lógica deve bloquear acesso mesmo que remoção física atrase; não depender de limpeza assíncrona para autorização. Propor retenção curta, por exemplo apagar metadados de sessão até 24 h após fim quando mecanismo gratuito e autorizado permitir, com regras testáveis e estratégia de limpeza sem ativar faturamento. Esse prazo é política candidata, não prova de serviço existente. Revalidar permissões e horário ao consumir; definir tolerância de clock medida e antirreplay sem confiar apenas em relógio do convidado.

Sinalização indisponível não concede novas permissões. PoC pode manter sessão já autenticada durante lease local válido, se política aprovada, e host continua podendo revogar localmente; documentar risco de revogação remota não recebida. Não renovar lease no modo offline silenciosamente. Um ticket manual pode servir ao laboratório; a experiência final automática depende de sinalização homologada.

### 8.3 Diagnóstico e privacidade

Logs técnicos: tipo de caminho, ID efêmero de sessão redigido, códigos de falha, versões, contadores de bytes/perdas/filas, durações e motivo de encerramento. Endereços completos só em inspeção local quando necessário; relatório compartilhado redige IP, porta sensível, IDs persistentes, convites e segredos. Não registrar payload UDP/TCP, áudio, transcrição ou pacotes de autenticação dos mods. Não gravar/analisar áudio para encaminhar.

Direto pode revelar IP ao peer; relay pode observar IP, timing e volume. Criptografia ponta a ponta protege conteúdo entre agentes, não anonimiza metadados. Política de retenção e operador do relay devem constar da escolha; não alegar invisibilidade de IP.

## 9. Conectividade, relay e zero-spend

Priorizar caminho direto. **Relay gratuito pode ser alternativa documentada e autorizada; gasto obrigatório, automático e ativação de faturamento permanecem proibidos.** Software open source não prova infraestrutura gratuita. Não exigir VPS/relay próprio, créditos temporários, rotação de contas, burla de quota ou relay público aleatório. Nenhuma opção é habilitada por este documento.

Uma biblioteca pode iniciar por relay gratuito autorizado e procurar direto em paralelo. Isso é aceitável se o fluxo real, operador, tráfego auxiliar e limitação forem observáveis e verificados. Não impor tentativa sequencial lenta nem prometer migração sem interrupção. Se o produto não tiver relay aprovado para distribuição, modo direto continua possível e falha deve ser clara quando não existir caminho permitido.

Toda dependência de sinalização, descoberta, STUN e relay deve ter entrada própria ou responsabilidades discriminadas na [matriz de dependências](AURORA_CONNECT_DEPENDENCIES.md): operador, finalidade, autenticação, licença, termos, quota, regiões, risco de cobrança, indisponibilidade, fonte/data e aprovação para teste versus distribuição. Serviço com excedente pago fica desabilitado sem bloqueio comprovado de cobrança; orçamento/alerta/contador do aplicativo não é tal bloqueio. Gratuidade de laboratório não autoriza produção. Terms/quotas desconhecidos não são preenchidos por inferência.

Avaliar Iroh primeiro, sem decisão antecipada; comparar Quinn com conectividade compatível e Pion ICE + quic-go apenas se vantagem justificar processo auxiliar. Não implementar todas. A [proposta de ADR](ADR/PROPOSED-aurora-connect-transport.md) fixa **Iroh 1.1.0** como referência documental de investigação, com toolchain e dependências a auditar; não adiciona pin ao Aurora. Ela registra APIs de streams/DATAGRAM e caminho WSS/TCP/TLS no cliente relay da tag, ainda sem teste Aurora. Não transpor essa evidência para qualquer versão `latest`. Em ICE + QUIC, validar reutilização do caminho/socket e demultiplexação da leitura; não tratar bibliotecas como encaixe automático. Se auxiliar for escolhido, incluir build, assinatura, updater, IPC e shutdown. Os detalhes e evidências pertencem à proposta de ADR.

Matriz de caminhos necessária: LAN; IPv6 direto com firewall; IPv4 alcançável; UDP hole punching; NAT duplo/CGNAT; redes distintas; UDP externo bloqueado. PCP/NAT-PMP/UPnP são opcionais apenas quando suportados, com consentimento, lease curto, remoção e falha recuperável. Mapear roteador doméstico não vence CGNAT por definição; IPv6 não elimina firewall.

Relay acessível somente por UDP não resolve bloqueio de UDP externo. Se a biblioteca oferecer TCP/TLS/WebSocket, registrar protocolo real e validar o caminho inteiro. Mesmo que a interface exponha DATAGRAM, uma perna confiável inferior pode reter mensagens por retransmissão: medir atraso/jitter de voz, filas vencidas e impacto de perda. Não prometer latência igual à direta nem declarar áudio confiável só por conectividade do jogo.

e4mc/World Host permanecem comparáveis por escopo, custo, termos, TCP/UDP, loaders e integração; uso de relay não é motivo de exclusão. Suporte a jogo/LAN não prova voz ou transporte genérico. Infraestrutura própria obrigatória continua adiada. Ausência de caminho direto e de relay permitido termina com motivo verificável; um timeout sozinho não diagnostica CGNAT.

## 10. Estados de interface baseados em evidência

| Eixo/estado candidato | Evidência mínima exigida | O que não comprova |
| --- | --- | --- |
| Preparação autorizada | Registro validado, aprovação local e configuração compatível/sem conflito | Serviço escutando. |
| Convidado autorizado | Convite válido, chave vinculada e política host aceita | Minecraft autenticado. |
| Transporte conectado | Handshake criptográfico e sessão aceita por ambos | Destino do jogo ou UDP do mod funcionando. |
| Caminho direto / relay gratuito | Evento/API da biblioteca informa caminho ativo e relay consta da allowlist aprovada | Prova de anonimato ou migração sem interrupção. |
| Caminho desconhecido | Biblioteca não expõe prova suficiente ou transição inconclusiva | Não rotular como direto por ausência de log de relay. |
| Servidor do jogo acessível | TCP conectado e, se implementada, consulta de status Minecraft válida | Jogador no mundo. |
| Jogador no mundo | Evento de jogo/servidor validado para instância e sessão, via integração homologada | Não usar apenas IPC Core conectado, processo JVM vivo ou bytes fluindo. |
| Ponte UDP pronta | Associação bidirecional validada com endpoint de teste ou serviço | Mod autenticado ou microfone funcional. |
| Voz conectada | Evento oficial de mod correlacionado + troca bidirecional esperada | Qualidade/audibilidade; API de conexão pode ter limites próprios. |
| Voz validada no teste | Mod conectado e verificação funcional autorizada entre participantes, sem gravação/análise pelo Aurora | Não generalizar resultado para outras versões/redes. |
| Serviço experimental | Combinação/caminho sem evidência de homologação | Nunca apresentar como pronto. |

Estados partem do backend e incluem sessão/revisão/timestamp monotônico; frontend não promove status por animação/timer. Com fonte de evidência indisponível, mostrar “não verificado”. Não recolocar telemetria removida do Companion para preencher UI.

Falha de voz não derruba automaticamente jogo. Distinguir mod ausente, versão não homologada, porta ocupada, destino indisponível, credencial/convite inválido, quota excedida, datagrama grande e falha de conectividade **quando houver evidência**. Caso contrário, erro genérico com ação de diagnóstico. O jogo pode exigir mod por política do próprio servidor; o launcher explica essa condição sem alterar a configuração do servidor.

O fluxo final a homologar é preparar → criar sessão → autorizar → negociar → validar jogo/serviços → entrar. Presença social, convite e sync por manifesto têm autorização e estados próprios. Não baixar pacote automaticamente nem abrir mundo só porque alguém aceitou convite.

## 11. Plano de verificação e evidências

Os testes abaixo são especificações, **não resultados executados**. Cada execução deve registrar commit, plataforma, versões de biblioteca/mod/Minecraft/loader/Java, hash de JAR, configuração relevante redigida, topologia, número de participantes, caminho observado, comando real, duração, critérios, resultado e arquivo de evidência. Ambiente externo exige autorização e garantia de não gerar custo. Mocks/loopback/LAN não comprovam travessia real de NAT.

### 11.1 Transporte controlado

| Grupo | Casos mínimos | Critério de aceite |
| --- | --- | --- |
| TCP integridade | Payload binário determinístico, vazio, >buffer, leituras/escritas parciais, ida/volta simultânea | Hash/tamanho iguais, ordem preservada em cada stream; nenhum byte entre conexões. |
| TCP concorrência | Status + jogo simulado + múltiplos convidados; leitura lenta | Streams independentes, backpressure mensurável e memória/handles dentro de limites definidos. |
| TCP ciclo de vida | Half-close, reset host/guest, cancelamento, app encerrado, destino recusado | Outra direção tratada corretamente; tarefas/sockets liberados no limite medido; erro específico sem derrubar terceiros. |
| Autenticação negativa | Peer/chave errado, certificado não esperado, convite alterado/expirado/revogado/repetido, sessão antiga | Nenhuma abertura de endpoint host nem pacote da aplicação encaminhado; motivo redigido. |
| Autorização negativa | Serviço inexistente, TCP solicitado como UDP, destino arbitrário, manifesto hostil | Negado antes de conexão; contagem de saídas host igual a zero. |
| UDP integridade/fronteiras | Mensagens 0/1/teto/teto+1, conteúdo binário, tamanhos alternados | Mesma mensagem integral ou descarte explícito; nunca truncamento/fusão. |
| UDP retorno/isolamento | ≥3 convidados com payloads identificáveis sintéticos, 2 associações por serviço e 2 serviços, respostas fora de ordem | Cada resposta chega só à associação correta; nenhum vazamento entre participantes/sessões. |
| UDP perda/reordenação | Injetar perda, duplicação, atraso e reordenação em fixture local | Sem crescimento ilimitado/retransmissão própria; sem reordenar artificialmente protocolo geral; contadores coerentes. |
| UDP MTU | Negociação ausente, teto menor que enquadramento, tamanhos extremos, mudança de caminho/limite | Serviço UDP indisponível com motivo; TCP preservado quando possível; zero truncamentos. |
| UDP recursos | Saturação, associação falsa, flood autenticado, ociosidade, porta reaproveitada, datagrama tardio | Limites antes de alocação, retorno antigo descartado, limpeza comprovada de sockets/tabela. |
| TCP+UDP | Tráfego TCP volumoso simultâneo a UDP e controle | Isolamento e contenção medidos; prazo de fila respeitado; política de conexões justificada por dados. |
| Encerramento | Revogar durante tráfego, reiniciar/crash, reconectar com novo epoch | Fluxos antigos negados, listener fechado, nenhum destino reaberto sem autorização. |

Fixtures determinísticas são endpoints controlados novos quando necessários; nomes/caminhos e comandos só serão fixados após auditoria. Payload de teste sintético não é gravação de voz. Não usar resposta de ping UDP do mod como prova de autenticação completa.

### 11.2 Configuração, Minecraft e voz

- Porta local já ocupada, IPv4/IPv6 inconsistentes, duas instâncias simultâneas, dois serviços de mesmo número TCP/UDP, porta de voz distinta da de jogo e porta runtime diferente da configuração persistida.
- Plano recusado; snapshot falhando; escrita interrompida; arquivo editado entre plano/aplicação; edição concorrente antes do restore; journal recuperado após crash. Aceite: nenhum dado do usuário perdido e conflito apresentado sem sobrescrita.
- Dedicado e mundo LAN, quando o mod/versão suportar; descoberta real, abrir LAN separado, host fecha mundo, jogo autenticado, status simultâneo, auto-join apenas onde implementado. Servidor ainda aplica autenticação/whitelist.
- Simple Voice Chat isoladamente: versão/hash selecionados, anúncio efetivo, API escolhida compilada contra a mesma versão, porta, limite de datagrama, login/saída/reconexão e voz bidirecional funcional. Testar jogador direto e Aurora se compatibilidade mista for anunciada.
- Plasmo Voice isoladamente: mesmos gates, com `[host.public]`/adaptador real da versão; não herdar aprovação do SVC. Testar LAN e misto apenas após provas correspondentes.
- Falha de voz mantém jogo; mod ausente/versão incompatível não cria falsa confirmação; credencial UDP inválida continua negada pelo mod. Não desativar criptografia/autenticação como preparação de teste.
- Relacionar comportamento por IP: todos vistos pelo host como agente/loopback não podem ganhar confiança administrativa nem identidade inventada; documentar efeito sobre bans/rate limits/plugins.

### 11.3 Rede real, custo e robustez

Executar cenário controlado em LAN, IPv4 público, IPv6 com firewall, NATs em redes distintas, NAT duplo/CGNAT e UDP externo bloqueado. Registrar topologia conhecida e evidência real; não concluir tipo de NAT por timeout. Separar direto, relay permitido e ausência de ambos. Para relay, testar indisponibilidade, credencial inválida, limite de quota e recusa de opção paga. A indisponibilidade de relay não significa regressão da política zero-spend.

Testar mudança de caminho, perda/reconexão de interface, suspensão/retorno, reinício host, encerramento convidado e sessão longa. Brasil–Brasil e Brasil–EUA são cenários explícitos, com consentimento dos participantes e ambiente gratuito autorizado; região anunciada por operador não prova rota real. Falta de ambiente registra **NÃO VERIFICADO**, sem fabricar sucesso.

Medir: tempo de preparação, handshake e conexão do jogo separadamente; RTT de rede; jitter/perda de datagramas sintéticos; bytes por direção e overhead; CPU/RAM/handles; uso de banda; filas/descarte; tempo de limpeza; estabilidade; impacto de download de chunks/carga do jogo sobre UDP. Para mod real, validar percepção/funcionamento por roteiro humano autorizado, sem gravar/analisar áudio. RTT de rede não é atraso de áudio, e atraso do servidor/MSPT não é ping; métricas do servidor precisam de fonte própria comprovada, sem recuperar telemetria removida.

Relatar amostra, duração, mínimo/mediana/p95 quando amostra suportar e falhas observadas; não inventar percentuais de sucesso, ganhos de ping ou latência universal. Metas finais de desempenho dependem de baseline e orçamento aprovado, não de número escolhido para marketing.

## 12. Incrementos, gates e rollback

1. Inventário/ameaças/custos/contratos: confirmar o existente; escolher versão de uma tecnologia e limites. O transporte antigo que funcionar não deve ser refeito por obrigação desta lista.
2. Menor ponte TCP autenticada e verificável: endpoints locais explícitos, chaves esperadas, serviço aprovado, cancelamento/limites e teste negativo. Fronteiras de controle/serviço já acomodam protocolo UDP.
3. Ponte UDP bidirecional controlada: associação/retorno, tamanho/enquadramento, limpeza e teste simultâneo TCP+UDP. Não depende de Minecraft/Core para provar transporte.
4. Conectividade entre redes e relay gratuito, apenas quando verificado/autorizado: caminho observado e falhas reais, incluindo UDP bloqueado.
5. Integração Minecraft/launcher: sessão, convite, endereços, estados reais, sincronização por manifesto e auto-join como capacidades separadas.
6. Homologar SVC e Plasmo individualmente: versão/loader/dedicado/LAN/caminho/endereço efetivo, cenário misto se prometido e recuperação de configuração.
7. Robustez, redes Brasil–Brasil/Brasil–EUA, regressão Core/Skins, privacidade e liberação gradual conforme gates R0–R5.

A ordem pode mudar para atacar o bloqueio real; escopo mínimo não apaga UDP/voz. Cada tarefa do backlog precisa de objetivo, escopo, dependências, arquivos conhecidos ou propostos, risco, teste, aceite, rollback, estado real e evidências. Aprovação TCP não aprova UDP, voz, relay nem R4 inteira. Recursos sem evidência permanecem experimentais e desativados para usuários por padrão.

Rollback comum: desativar capacidade de Connect, invalidar sessão/convites, cancelar conexões, fechar listeners/associações, limpar leases de roteador e restaurar alterações próprias por journal sem apagar divergências. Retirar dependência/feature em mudança revisável se o incremento for rejeitado. Não fazer reset destrutivo, apagar alterações locais, publicar artefatos ou modificar serviços externos como parte de rollback documental.

## 13. Decisões pendentes que bloqueiam promessas específicas

| Decisão | Evidência necessária antes de fechar |
| --- | --- |
| Biblioteca/versão do transporte | Inventário/lockfile, API da versão, teste autenticado, licenças, plataforma e custo; proposta de ADR revisada. |
| Relay para distribuição | Condições efetivas do operador, quota/limitação/região, política de cobrança bloqueada e autorização para uso do launcher; alternativa de testes não basta. |
| Enquadramento UDP final | Tamanho real do cabeçalho, limites APIs/caminhos, testes fronteira/retorno e negociação. |
| Datagramas de voz maiores | Histograma por mod/config/caminho; sem fragmentação validada, compatibilidade permanece limitada. |
| Uma ou duas conexões | Medição TCP+UDP/controle sob carga/perda, justiça entre participantes e custo de handshakes. |
| Identidade peer↔jogador | Contrato verificável sem tokens, sem confiar em UUID informado; necessário para adaptação servidor por participante. |
| Anúncio SVC em LAN/misto | Artefato e API reais, ordem de eventos e ponte reservada antes do anúncio; teste por alvo. |
| Anúncio Plasmo em LAN/misto | Fonte versionada/add-on e endpoint efetivo; `[host.public]` sozinho não resolve todos os clientes. |
| Sinalização de produto | Regras positivas/negativas, grants, expiração/retenção e gratuidade comprovada; ticket manual é apenas laboratório. |
| Portabilidade | Build/distribuição e shutdown no sistema selecionado; nenhuma promessa multiplataforma por dependência compilar. |
| Desempenho e capacidade | Baseline e carga real, regras de fila/limites calibradas, critérios por cenário com amostra registrada. |

## 14. Aceite desta especificação documental

Concluída a documentação não significa concluído R4. A revisão deve conseguir localizar: política que permite relay gratuito com zero-spend; separação dos quatro modos de uso; bytes TCP confiáveis; datagramas UDP e retorno isolado; limites/MTU; duas partes do problema de voz; fontes e versões com lacunas; segurança/UX/testes e rollback. O futuro relatório de implementação deve declarar explicitamente quais dessas hipóteses foram confirmadas ou rejeitadas e quais evidências foram geradas.

## 15. Registro de fontes oficiais consultadas

Todas as linhas abaixo foram consultadas em **29/09/2026**. Data é de consulta, não garantia de estabilidade futura. Configurações web sem versão fixa devem ser verificadas contra o artefato selecionado; não copiar API `latest` para lockfile antigo. Para Iroh, Quinn, Pion, e4mc/World Host, termos e infraestrutura, ver o registro complementar da [matriz de dependências](AURORA_CONNECT_DEPENDENCIES.md) e da [proposta de ADR](ADR/PROPOSED-aurora-connect-transport.md).

| Referência | URL | Uso/limitação |
| --- | --- | --- |
| QUIC DATAGRAM | https://www.rfc-editor.org/rfc/rfc9221.html | Contrato normativo; não demonstra integração Aurora. |
| SVC configuração | https://modrepo.de/minecraft/voicechat/wiki/server_config | Campos, contexto dedicado e tamanho configurável; sem tag de artefato. |
| SVC proxy | https://modrepo.de/minecraft/voicechat/wiki/proxy_setup | Proxy genérico UDP e plugins oficiais são cenários distintos. |
| SVC versões | https://modrepo.de/minecraft/voicechat/wiki/supported_versions | Disponibilidade por Minecraft/loader; não é homologação Aurora. |
| SVC downloads oficiais | https://modrepo.de/minecraft/voicechat/downloads | Origem oficial a usar na seleção; nenhum download foi realizado. |
| SVC licença observada | https://github.com/henkelmax/simple-voice-chat/blob/1.20.1/license | Direitos reservados no branch consultado; conferir artefato/API e autorização de distribuição. |
| SVC índice API | https://voicechat.modrepo.de/ | Identifica API 2.6.20 na consulta; JAR de teste ainda não fixado. |
| SVC anúncio por jogador | https://voicechat.modrepo.de/de/maxhenkel/voicechat/api/events/VoiceHostEvent.html | `getPlayer`, `getVoiceHost`, `setVoiceHost` documentados. |
| SVC socket cliente | https://voicechat.modrepo.de/de/maxhenkel/voicechat/api/events/ClientVoicechatInitializationEvent.html | Injeção oficial de socket candidato. |
| SVC contrato socket | https://voicechat.modrepo.de/de/maxhenkel/voicechat/api/ClientVoicechatSocket.html | Métodos reais; implantação/homologação pendentes. |
| SVC estado cliente | https://voicechat.modrepo.de/de/maxhenkel/voicechat/api/events/ClientVoicechatConnectionEvent.html | Sinal de conexão, não qualidade de áudio. |
| Plasmo endereços | https://plasmovoice.com/docs/server/advanced/ | Distinção bind/anúncio e `[host.public]`; sem tag de artefato. |
| Plasmo instalação | https://plasmovoice.com/docs/server/installing/ | UDP separado de TCP. |
| Plasmo produto | https://plasmovoice.com/ | Declara LAN/plataformas; não prova integração Aurora. |
| Plasmo release | https://github.com/plasmoapp/plasmo-voice/releases/tag/2.1.17 | Release 2.1.17 e assets observados; não selecionados. |
| Plasmo licença da tag | https://github.com/plasmoapp/plasmo-voice/blob/2.1.17/LICENSE | LGPLv3; componentes/obrigações do artefato a conferir. |
| Plasmo add-on API | https://plasmovoice.com/docs/api/ | Módulos e mecanismo oficial de add-on. |
| Plasmo API fixada para pesquisa | https://dokka.plasmovoice.com/older/2.1.17/index.html | Evita usar default 2.2.0-SNAPSHOT como prova de 2.1.17. |
| Plasmo extensão cliente | https://dokka.plasmovoice.com/older/2.1.17/client/su.plo.voice.api.client.event.socket/-udp-client-connect-event/index.html | Oportunidade de custom UDP; mutadores/fonte exatos a conferir. |
| Plasmo evento servidor | https://dokka.plasmovoice.com/older/2.1.17/server/su.plo.voice.api.server.event.connection/-tcp-packet-send-event/index.html | Evento existe; anúncio individual seguro não comprovado. |
| Plasmo estados cliente | https://dokka.plasmovoice.com/older/2.1.17/client/su.plo.voice.api.client.event.socket/index.html | Eventos publicados para futura correlação UX. |
