# Aurora Connect — admissão social local, incremento 88B

Data: 25/10/2026. Estado: **PROTÓTIPO**, **IMPLEMENTADO MAS NÃO HOMOLOGADO** para publicação. Contrato: [ADR-031](engineering/ADR/ADR-031-social-local-admission.md). Comandos, evidências e checks finais: [tarefa 88B](task-aur-r4-connect-social-88b.md). Complementa [88A](aurora-connect-social-key-binding-88a.md), [87](aurora-connect-social-87.md) e R4-005/008, sem fechar esses IDs ou alterar a matriz e os gates.

## Capacidade acrescentada

Dois processos nativos locais agora conseguem partir de contas Aurora autenticadas nos emuladores, provar posse de suas chaves, obter consentimento de hospedagem e uma autorização individual assinada, e transportar TCP real sem o usuário copiar um convite entre eles. A mesma chave privada efêmera permanece no processo entre a prova 88A e o transporte autenticado.

O recorte é uma conta anfitriã, uma convidada, uma vaga, serviço `game`/TCP e uma fonte `127.0.0.1:porta`. São admitidas sessões sociais `public` ou `friends`, somente com política `open`; sua projeção fica congelada por até 60 segundos. O teste usa uma fonte de eco TCP, **não um servidor Minecraft**.

O runner possui os processos e a autoridade local, faz a troca interna de objetos e executa a autorização explícita da sua própria fonte de teste. Isso não é uma nova tela: **Entrar continua desabilitado**, sem acionamento automático do Tauri ou integração de produção. A admissão por pedido, vários convidados e um catálogo remoto continuam fora do recorte.

## Cadeia de autorização

1. O runner cria o servidor opt-in local e recebe sua chave pública diretamente do objeto que criou. O pin, a origem HTTP e o projeto não vêm de um renderer ou do grant recebido.
2. Os clientes nativos recebem conta/sessão selecionadas e conservam chave, operação, geração e prazo originais. A conta desses argumentos vem do login Auth do runner; a autoridade reconfere a autenticação independentemente.
3. O anfitrião recebe seu desafio 88A e produz a prova com sua chave RAM. Revê fonte, visibilidade, serviço, capacidade e prazo. Somente a aprovação do digest exato assina a `HostPolicy`.
4. A autoridade recebe a prova original, não um recibo enviado pelo cliente. Confere conta, propriedade, contexto, assinatura da política, cadastro e sessão atuais antes de registrar uma política por anfitrião/sessão.
5. Antes de receber seu desafio, o convidado recebe uma expectativa imutável do mesmo runner: UID/chave/operação/geração do host, política/digest e digest da sessão. Esses valores vieram da política anfitriã já aprovada/registrada; não são aprendidos do grant.
6. Após a prova 88A do convidado e nova leitura autorizada, a autoridade reserva a única vaga e emite `AdmissionGrant` e a primeira `ContinuityLease`. A conta e a chave convidadas precisam ser diferentes das anfitriãs. Prova consumida ou reserva que falha não se tornam reutilizáveis.
7. Ambos os clientes verificam pin, assinatura, contexto completo e prazos. O host emite internamente a `SessionInvitation` existente com o serviço e os prazos absolutos aprovados. O guest confere seu pin host, sessão, serviço e vencimentos antes do handshake TLS existente.
8. A continuidade depende de autenticação, cadastro e leitura atual pelo token do convidado. Bloqueio, restrição, falha, cancelamento ou vencimento tornam o grant terminal; uma resposta posterior não ressuscita a conexão.

O recibo 88A continua com `transportAuthorized:false`. A nova autoridade não o promove a passe. O helper interno `proveRetained` executa a mesma verificação/consumo único e transfere o deadline monotônico original exclusivamente entre componentes do mesmo processo. A rota HTTP 88A continua devolvendo apenas seu recibo público; nenhum cliente escolhe ou recebe esse metadado monotônico.

## Objetos, prazos e limites

Todos os objetos são fechados, com campos limitados, inteiros positivos seguros e codificação canônica por comprimentos UTF-8 em u16 big-endian. Domínios distintos impedem reutilizar uma assinatura de posse, política, admissão ou continuidade em outro protocolo. Cada pacote recebido tem limite de 8192 bytes; as rotas 88A conservam seu limite anterior de 4096 bytes.

| Objeto | Assinante e contexto | Prazo/limite |
| --- | --- | --- |
| `HostPolicy` | Chave do host; conta/operação/geração/sessão/digest, visibilidade, `open`, `game`, TCP, digest da fonte e uma vaga | Não supera o desafio/preparo original; até 32 políticas por processo, sem expulsão de política viva |
| `AdmissionGrant` | Chave efêmera da autoridade; política, sessão, ambos os UIDs/chaves/operações/gerações | Admissão até 10 s; sessão até o menor vencimento original de host, guest e política |
| `ContinuityLease` | Mesma autoridade; instância, grant, digest da política e sequência crescente | Até 3000 ms, nunca além do prazo original da sessão |

A fonte é ligada por SHA-256 de `aurora/connect/social-source/loopback/v1`, byte zero e SocketAddr IPv4 canônico `127.0.0.1:porta`. O anúncio social não fornece endereço de rede. As preimagens e a ordem completa dos campos estão na ADR-031.

A autoridade limita a continuidade a quatro verificações por segundo/grant e 256 por minuto/processo, além dos limites globais e de autenticação. Cada consulta tem timeout de 5 s e cancelamento. O marco de frescor é capturado antes da leitura, digest e assinatura; essas esperas não criam mais três segundos. Políticas/grants conservam o menor deadline monotônico original e reduzem, quando necessário, o vencimento civil emitido. Recuar o relógio dentro do intervalo assinado não amplia a sessão.

## Monitoramento e encerramento

O host acompanha a lease em um timer de 20 ms independente da leitura do stdin. Ao perder continuidade, chama `Host::revoke`, aguarda shutdown dos workers/endpoint e verifica rebind dos sockets UDP próprios. O guest observa o fechamento do transporte, encerra a ponte TCP e o endpoint e verifica rebind de TCP/UDP sem depender de um `stop` posterior enviado pelo runner.

Sequência repetida, assinatura/contexto inválidos ou continuidade já vencida são terminais. A lease não renova o prazo da sessão. O monitor usa o objeto verificado e seus deadlines originais, sem reconstruir autoridade repetidamente a partir dos bytes assinados.

A política de lease é de 3000 ms; há ainda o intervalo de observação do timer e a latência de encerramento. Isso não é bloqueio instantâneo ou SLA distribuído. Na rodada final, o fechamento do TCP ocorreu em **2760 ms após iniciar o bloqueio** e **2764 ms após iniciar a perda do backend**. A primeira rodada registrou 2792 ms e 2786 ms, respectivamente, e permanece separada. São medições dessas rodadas, não garantia geral de latência. O teste possui limite externo de 7 s e exige a causa esperada e prazo de sessão ainda vigente para não atribuir expiração normal a revogação.

Durante bind/dial inicial, o helper retém a operação pendente para recolher um endpoint tardio antes de publicar disponibilidade. A função de bind da biblioteca não fornece aqui um prazo cancelável próprio; portanto a política de 3 s para conexão ativa não constitui prova de encerramento de startup em três segundos.

O helper possui um único leitor stdin com fila limitada a oito linhas de até 8192 bytes. EOF, stop e cancelamento não criam um leitor novo por comando. Um leitor de console bloqueado termina com o processo depois da limpeza de rede; o runner fecha stdin e aguarda o processo de sua propriedade.

## Integração local observada

O runner usa o Auth Emulator e o SDK/Firestore reais, reaproveita o serviço social do launcher e não substitui as decisões de amizade, bloqueio ou sessão por respostas simuladas. O servidor segue fixado no projeto `demo-aurora-social`, Auth `127.0.0.1:9187` e Firestore `127.0.0.1:8187`. As três rotas novas só existem com `admission:true`: `/v1/social/host/register`, `/v1/social/join` e `/v1/social/continuity`.

| Cenário real local | Resultado |
| --- | --- |
| Sessão pública | Duas contas Aurora, política aprovada, admissão e dois processos Rust reais; eco de 1 MiB por direção com EOF; fluxo adicional ativo interrompido após bloqueio real da conta convidada |
| Sessão para amigos | Pedido/aceitação reais pelo SDK, política `friends`, mesma prova TCP; perda do servidor de autoridade interrompe continuidade e TCP |
| Bloqueio antes do ingresso | Bloqueio após a prova de chave e antes de join; HTTP 403, zero bytes e zero conexões na fonte TCP |

Nos cenários ativos, o runner não injeta stop após a retirada. Exige `social_host_stopped` com `reason=continuity_expired`, admissão observada, cleanup/rebind verdadeiros; guest deve terminar com `reason=connection_closed`, cleanup/rebind verdadeiros. Ambos os processos terminaram com exit 0 e sem encerramento forçado. O runner também conseguiu reabrir a porta TCP convidada. A limpeza de fixtures e o encerramento global dos emuladores são registros separados.

Resultado TAP final: **4 PASS**, correspondentes a três cenários mais o agrupador, em **9666,7213 ms**. A primeira rodada obteve os mesmos quatro PASS em 12224,7953 ms; não somar como oito cenários. Não são quatro jogos ou quatro capacidades de transporte distintas. Nenhum desses testes abre mundo Minecraft, reproduz áudio ou envolve outra internet.

Outras evidências executadas: seis unitários Rust PASS em 0,35 s; quatro testes reais de CLI/preparo na rodada final PASS em 0,71 s, com compilação em 1 min 22 s; 41 testes Node combinados PASS em 467,7932 ms; regressão da integração 88A com oito PASS em 1090,9628 ms. A suíte edge completa passou nos 196 testes em 9749,9944 ms e já inclui os 41 focais. O build inicial do novo helper passou em 18,37 s; a primeira rodada CLI teve os mesmos quatro PASS em 0,68 s. Não somar repetições.

Clippy inicialmente apontou `collapsible_if` no Drop do leitor; o integrador aplicou let-chain e corrigiu a ordem de exports indicada pelo fmt. Clippy connect-lab com todos os targets passou em 13,04 s; desktop com todos os targets/features passou em 28,77 s; TypeScript strict para ambos os módulos, fmt final e diff check também passaram. O rebuild foi novamente exercitado pelos testes CLI e integração reais; o resultado inicial não foi reatribuído ao novo binário.

Helper final `apps/connect-lab/target/debug/aurora-connect-social-client.exe`: 23.484.928 bytes, SHA-256 `6c6fea4b69872fe5cc38d9a9b78c71a71e84d14b2f961369b4b9fa53010966b2`. Primeiro helper, de mesmo tamanho: SHA-256 `ca28c785d48ea17133c2f55cdf7a5f71623eb78e0921f4714048babb410f055f`, associado somente à primeira rodada. Ambos são artefatos de laboratório, não um novo executável gráfico/instalador do launcher. Logs e comandos estão na tarefa 88B.

Ao fechar, a sessão própria dos emuladores recebeu Ctrl+C e retornou exit 1. O sistema confirmou depois ausência dos listeners nas portas 8187/9187/4487/4587/9150/42888 e dos processos Node/Java/helpers identificados na tarefa. É interrupção seguida de verificação no sistema, sem alegação de saída normal dos emuladores ou rebind dessas portas globais. Os clientes Rust têm suas provas de exit 0/rebind separadas. O checkout principal, a matriz ativa e a proposta de voz não rastreada foram conferidos e preservados.

## Situação por componente

| Componente | Estado de 88B |
| --- | --- |
| TCP | Nova evidência real local de admissão social, 1 MiB por direção/EOF e fechamento ativo |
| UDP | Nenhuma ponte de aplicação UDP ou voz acrescentada; sockets UDP do transporte QUIC não equivalem a essa validação |
| Conexão direta | Caminho loopback local; nenhum novo ensaio IPv4/IPv6 externo, NAT ou CGNAT |
| Relay | Não usado, contratado ou habilitado |
| Minecraft | Não exercitado; fonte de eco não comprova runtime, mundo, jogadores ou Core/Skins |
| Convites/interface | Troca interna real no runner; nenhuma cópia manual no ensaio, nenhum novo Entrar gráfico ou autoapprove Tauri |
| Amigos/público | SDK/regras reais usados nos cenários locais; nenhum catálogo de produção publicado |
| Simple Voice Chat | Sem nova prova de áudio, transporte, endereço ou recuperação |
| Plasmo Voice | Sem nova prova de áudio, transporte, endereço ou recuperação |

Ainda faltam integração da ação social no mesmo `ConnectManager/owner` e barreiras de conta, descoberta/fonte adequada ao jogo, contrato de produção e validações externas. O teste de interrupção de heartbeat com backend saudável, injeções de lease inválida durante tráfego e startup tardio controlado não são apresentados como ensaios reais executados nesta rodada. Os testes unitários e as regras de código pertinentes não substituem esses ensaios.

## Isolamento e rollback

O Auth Emulator admite tokens sem assinatura por definição; o adaptador local não é autenticação criptográfica de produção. Não foi adicionado fallback `alg=none` no Worker, credencial de produção, URL/pin arbitrário de renderer, CORS aberto ou endpoint remoto. O renderer e o build normal não ganham capacidade de ingresso por este incremento.

Rollback: parar somente autoridade/helpers/fixtures próprios e reverter 88B, mantendo 88A e seus documentos históricos, logs e demais worktrees. Nenhum push, deploy, publicação, instalador, faturamento, firewall, configuração pessoal ou ampliação da matriz faz parte desta implementação.
