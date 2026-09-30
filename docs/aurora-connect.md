# Aurora Connect — laboratório local executável

Estado: **PROTÓTIPO**, fora do desktop e desativado para usuários. Base alpha.14, Core 1.1.5/Skins 0.3.1 preservados. [Execução/evidências](task-aur-r4-connect.md), [contrato canônico](engineering/AURORA_CONNECT_SPEC.md), [voz](aurora-connect-voice.md).

Incremento20: três ensaios com Minecraft 1.21.1/Fabric real e dois clientes sintéticos passaram LOGIN/CONFIGURATION/PLAY, posição/chunks e presença recíproca. Cada ensaio usa quatro agentes em dois pares independentes, contagem 2→0 e encerramento normal; somente loopback, sem mods/GUI/contas. [Procedimento e limites](aurora-connect-runtime.md#dois-clientes-de-protocolo--vigésimo-incremento). Não homologa redes externas, clientes mistos ou R4.

Incrementos25/26: SVC2.6.22 e Plasmo2.1.17 passaram separadamente pelo anúncio TCP e controle UDP real, cada um com um cliente sintético e dois agentes. SVC valida AuthAck/CheckAck AES-GCM; Plasmo valida Ping com credencial de sessão, sem MAC/AEAD próprio nesse pacote. QUIC mantém a cifra do transporte. Ambos tiveram stop normal e restauração nativa revisada; áudio e GUI continuam pendentes. [Execução atual](aurora-connect-runtime.md#controle-plasmo-pela-ponte--vigésimo-sexto-incremento).

## Preparação de dois computadores

Há roteiros separados para [a mesma rede LAN](aurora-connect-lan.md) e [redes diferentes por IPv6 candidato](aurora-connect-internet.md), com kit privado revisado, verificação de hash, papéis e encerramento. `scripts/connect-network-preflight.ps1` lê somente inventário local e emite contagens/booleans sem endereços. Em30/09, o usuário executou o kit32 em outro computador/outra internet: TCP1MiB por direção e10UDP com2origens passaram, teto negociado1264B. Convidado encerrou; anfitrião/echo tiveram exit0 e portas livres observados aqui. Direto externo **PARCIAL** nessa única amostra; não prova NAT/CGNAT, áudio, jogo, carga simultânea ou outras redes. [Registro33](task-aur-r4-connect.md#trigésimo-terceiro-incremento--conexão-direta-entre-duas-internets). Nenhuma intervenção humana adicional é necessária para os próximos trabalhos locais.

Incremento32 corrigiu uma configuração ignorada pelo Iroh: QNT permanece ativo, com32candidatos/8caminhos, separado de relay/lookup/observed reports desligados. Pode haver negociação de caminhos antes do Hello Aurora, com destinos de serviço ainda protegidos. [Limites efetivos](engineering/AURORA_CONNECT_SPEC.md#61-configuração-efetiva-do-laboratório--incremento32). O incremento33 usou esse mesmo binário no segundo computador; não instrumentou migração de caminhos.

## Convite manual assinado — experimental e somente loopback

Os comandos `host-invite-plan`, `guest-invite` e `guest-invite-tcp` são um fluxo separado do pin/lease manual anterior. Um convite autoriza **uma admissão**, com chave do convidado, sessão, serviços e prazo até10min, assinatura Ed25519 e aprovação do digest. Receber o JSON não substitui possuir a chave privada do convidado. Reentrar exige novas execuções/chaves/convite; não há persistência ou identidade social. [Contrato e limites](engineering/AURORA_CONNECT_SPEC.md#81-convite-manual-assinado--laboratório).

Roteiro automatizado próprio, com dois agentes e endpoint controlado:

~~~powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/connect-lab-smoke.ps1 -Invitations
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/connect-lab-smoke.ps1 -SkipBuild -Invitations -AuthorizedDownstream
~~~

`-SkipBuild` exige binário já compilado/revisado. Script prepara plano sintético novo, prova rejeição de hash incorreto, mantém convite apenas em RAM/stdin e imprime contagens redigidas. Encerramento envia stop ao convidado, host e echo; exige exit0 e `CLEANUP_PASS`. Não registra convite, chave, IP ou payload no relatório. Configurações pessoais não participam; plano novo permanece na pasta ignorada. Resultados realmente executados ficam no task; um roteiro ainda não executado não é evidência.

Ensaio manual preparado, **no mesmo PC**, usa o exemplo existente `apps/connect-lab/examples/host-downstream.json`, portas42000(transporte),42001(TCP destino),42002(UDP destino),42010/42011(pontes). Todos os terminais partem desta worktree. No terminal do endpoint, executar `./apps/connect-lab/target/debug/aurora-connect-lab.exe echo-downstream 127.0.0.1:42001 127.0.0.1:42002 600`. No anfitrião, executar `plan apps/connect-lab/examples/host-downstream.json` com o mesmo executável; conferir serviços/política/prazo e passar o hash retornado a `host-invite-plan apps/connect-lab/examples/host-downstream.json <hash-revisado>`. Hash é saída real a preencher, nunca copiar de outro plano.

No convidado, iniciar `guest-invite <peer-publico-do-host> 127.0.0.1:42010 127.0.0.1:42011 600`. Copiar seu peer público para o stdin do anfitrião, que emitirá o convite. Entregar o JSON completo ao stdin do convidado; conferir prévia, pin, serviços, expiração e listeners; enviar `approve <digest-da-previa>`. Cada prompt expira em60s ou antes pelo convite. Após guest_ready, `probe-downstream 127.0.0.1:42010 127.0.0.1:42011` comprova somente bytes sintéticos. Não usar esse convite em outro computador: este incremento recusa endereço externo.

Encerrar com `stop` primeiro no convidado, depois anfitrião e endpoint, aguardando eventos stopped/exit. EOF em prompt pendente recusa sem conexão; Ctrl+C cancela. Revogação/expiração do host também fecha o convidado. Porta ocupada falha, nunca encerra proprietário alheio. Reinício descarta chaves/convite; não reutilizar a mesma identidade host para tentar reconstruir consumo em memória. Nenhuma configuração de jogo, EULA ou mod muda neste ensaio. Convites completos e pins não pertencem a logs compartilhados; devolver somente PASS/código fixo/contagens/cleanup.

Regressão de retomada: `cargo test --offline --locked --manifest-path apps/connect-lab/Cargo.toml --test invitation_resumption -- --nocapture`. Cinco casos usam Iroh real e exigem0-RTT aceito, recusando replay/credencial inválida/revogação/expiração sem atingir os canários TCP/UDP. O resumo não imprime convites/pins. O teste não mede a janela temporal pré-TLS; [limite da evidência](engineering/AURORA_CONNECT_SPEC.md#81-convite-manual-assinado--laboratório).

## O que este executável faz

Dois agentes autenticam as chaves públicas esperadas usando Iroh 1.3.0 e QUIC cifrado. O host concede uma lease de laboratório ao peer convidado, sessão e serviços explícitos, com prazo e revogação. `guest` pede IDs `game` (TCP) e `aux` (UDP), revisão 1; `guest-tcp` pede somente `game`, anunciando apenas `reliableStream`. Os destinos aprovados são literais loopback do host; o convidado não escolhe IP/porta do destino.

Cada TCP usa stream próprio, buffers limitados, backpressure e half-close. UDP usa DATAGRAM autenticado com cabeçalho de 24 bytes e socket host exclusivo por associação; o retorno fica vinculado à conexão autenticada, serviço, associação e geração. IDs não são autorização. O conteúdo é opaco; não há voz própria, captura de áudio ou bypass de autenticação Minecraft/mod.

Transporte é somente direto com endereço manual. Relay, descoberta pública, portmapper e sinalização em nuvem estão desligados. Não há VPN, driver, instalação do launcher, cobrança ou alteração de firewall/roteador. Loopback não demonstra LAN, NAT traversal ou internet. Pin público e endereço são dados de conexão a trocar privadamente entre participantes; nenhuma chave privada é impressa ou persistida.

## Reproduzir neste computador

Incremento35 acrescenta uma prova local da fronteira UDP, separada do probe antigo. Requer binário32 já revisado/hash fixado e Node24.8; desligada por padrão:

```powershell
node apps/connect-client-lab/boundary-smoke.mjs --local-lab
$env:AURORA_CONNECT_BOUNDARY_TEST_REAL = '1'
node --test --test-concurrency=1 apps/connect-client-lab/boundary-smoke.test.mjs
```

O helper cria somente plano novo ignorado e endpoints loopback próprios, confere hash/serviços exatos antes de autorizar o peer e inicia dois agentesCLI. Teto1264 é aprovado no serviço, não MTU física forçada. Quatro ecos1264B, duas tentativas1265B, zero chegada excessiva ao endpoint em janelas300ms, válido posterior em cada origem e TCP1MiB/direção durante UDP passaram. Encerramento normal e rebind das pontes comprovados;7testesPASS sem skip quando opt-in. Sem opt-in os três casos reais são skips explícitos. Não executa Minecraft/mod/áudio/rede externa; retorno acima do teto ainda é lacuna. Sem alvo/binário/path arbitrários, firewall ou configs pessoais; stop/cancel fecha apenas filhos/sockets próprios e preserva o plano. ReapOS pode exceder prazo de parada graciosa. [Evidência35](task-aur-r4-connect.md#trigésimo-quinto-incremento--fronteira-udp-e-tcp-concorrente-em-loopback).

Diretório de execução:

```powershell
Set-Location C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect
cargo test --locked --manifest-path apps/connect-lab/Cargo.toml
cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path apps/connect-lab/Cargo.toml --all-targets -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/connect-lab-smoke.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/connect-lab-smoke.ps1 -AuthorizedDownstream
node --test apps/connect-voice-tools/inspect.test.mjs
```

O smoke compila somente o executável de laboratório e inicia três processos filhos ocultos: endpoint echo TCP/UDP, host e convidado. Portas loopback são efêmeras. Troca somente pins públicos por stdin; chaves privadas ficam em memória. Um quarto processo `probe` compara 1 MiB em cada direção TCP e dez datagramas (0, 1, 32, 512 e 1024 bytes em duas origens locais). Não executa Minecraft nem mod.

Saída esperada: JSON `result=PASS`, `agents=2`, `topology=same_machine_loopback` e `CLEANUP_PASS`. Os tempos medem ida/volta da aplicação de teste, **não** latência de áudio nem RTT puro de rede. Pico de memória/CPU são observações desses processos debug; não SLA, benchmark de jogo ou memória exclusiva dos buffers. Se uma porta for ocupada entre reserva e bind, falha sem encerrar seu proprietário. Saída compartilhável não inclui pins, endereços, payloads ou dados de conta.

Com `-AuthorizedDownstream`, o script cria um plano sintético próprio em `.aurora/connect-validation`, confere preview/hash, comprova recusa de hash não aprovado antes de iniciar o host e só então autoriza o peer do ensaio. O endpoint controlado responde oito datagramas de 512 bytes para cada pedido de um byte. Resultado esperado: duas origens, dois pedidos, 2 bytes enviados, 8.192 recebidos em 16 respostas íntegras, além do mesmo TCP. Os tempos são pedido–cada resposta, não RTT de cada pacote. A fixture permanece ignorada e inalterada; nenhuma configuração pessoal ou chave é persistida.

## Consulta de status de jogo pelo listener TCP — experimental

`minecraft-status <loopback-tcp-bridge> <handshake-host> <handshake-port> <protocol-number>` faz uma conexão TCP local, handshake com intenção STATUS, pedido de status e ping/pong correspondente. Não faz login, abre LAN, entra no mundo ou envia credenciais. O host do handshake é dado do protocolo para roteamento virtual: não é resolvido por DNS nem muda o endereço de conexão. O listener por si só não prova autenticação Connect; o caller deve associá-lo à ponte autenticada e à sessão corretas.

Limites: timeout total de cinco segundos na CLI, JSON até 64 KiB e 32.767 unidades UTF-16, framing conferido antes de alocar, sem retries. Recusa destino externo/porta zero, host acima de 253 bytes ou com caracteres de forwarding/controles, protocolo zero/negativo, VarInt inválido, frame/tipo errado, truncamento e pong divergente. Não interpreta eventos de rich text nem decodifica favicon. Resumo contém somente protocolo informado/relatado, igualdade, contagens relatadas e durações; nenhum MOTD, nome/UUID de jogador, imagem ou host é retornado. As contagens são dados declarados pelo servidor.

Um teste usa Host/Guest reais e uma fixture de protocolo independente no destino TCP; outro executa a CLI. A fixture não é Minecraft. O comando abaixo é um **procedimento preparado para runtime ainda pendente**: pressupõe um servidor já autorizado no destino de `game`, bridge existente em `127.0.0.1:42010`, handshake local `localhost:25565` e par 1.21.1. Não executar sobre o endpoint echo, que não responde ao protocolo de status.

    ./apps/connect-lab/target/debug/aurora-connect-lab.exe minecraft-status 127.0.0.1:42010 localhost 25565 767

Sucesso emite `minecraft_status_received` após status e pong válidos, com `runtimeHomologated=false` e `worldJoined=false`; diferença de protocolo fica explícita. A consulta fecha sua conexão ao concluir/falhar/cancelar e não encerra outras conexões do túnel. O encerramento do host/convidado segue `stop`/Ctrl+C/prazo. Sem mutação, arquivos ou snapshot adicional. Comandos/resultados efetivamente executados estão no [task](task-aur-r4-connect.md).

Referências de wire consultadas em 29/09/2026: [MCProtocolLib fixado para 1.21/protocolo 767](https://github.com/GeyserMC/MCProtocolLib/blob/e71d7df8c127b7196fd81dd392034d7d97acbb4e/protocol/src/main/java/org/geysermc/mcprotocollib/protocol/codec/MinecraftCodec.java), [handshake na revisão consultada](https://github.com/GeyserMC/MCProtocolLib/blob/19783c29ece24bc3f07f8ff08628549527e3de20/protocol/src/main/java/org/geysermc/mcprotocollib/protocol/packet/handshake/serverbound/ClientIntentionPacket.java) e [registro 1.21.1/protocolo 767](https://github.com/PrismarineJS/minecraft-data/blob/d56346a4e32defcd28d9ba6750957c12001e78d6/data/pc/1.21.1/version.json). Não adiciona nenhuma dessas bibliotecas, nova versão à matriz ou homologação de mod.

## Plano explícito de serviços do laboratório

Para um destino Minecraft já autorizado, existe o roteiro `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/connect-minecraft-status-smoke.ps1 -GamePort 43001`. Ele inicia os dois agentes ocultos, cria/revisa por hash um plano próprio com apenas `game/tcp`, troca pins por stdin e executa `minecraft-status` por `guest-tcp`. Não inicia ou encerra o servidor de destino; a vida desse processo pertence ao caller. Protocolo esperado 767; divergência é falha. Saída redigida só após status/pong e encerramento normal de ambos os agentes, com `runtimeHomologated=false`. Eventos limitados a 16 KiB/30 s e cleanup de filhos próprios em `finally`. `-SkipBuild` requer binário já compilado. Foi testado com uma fixture de protocolo independente, **não com Minecraft**, conforme [task](task-aur-r4-connect.md) e [preparo runtime](aurora-connect-runtime.md).

`guest-tcp <host-public-id> <host-direct-address> <session-id> <local-tcp> <seconds>` conserva pin/sessão/lease/revisão e a confirmação `ready` por stdin. `guest_ready` informa somente listener TCP; a autorização de `game` é exercida na conexão/probe subsequente. O transporte QUIC continua usando UDP; não confundir ausência de serviço UDP de aplicação com TCP externo capaz de atravessar uma rede que bloqueia QUIC.

O comando convencional `host` mantém `ReplyBudget`. Para autorizar o perfil assimétrico, `plan` lê um JSON local de até 16 KiB, apresenta o estado antes/depois e seu SHA-256. `host-plan` exige esse hash, relê os bytes e recusa qualquer diferença antes de abrir transporte. A revisão é dos bytes exatos, incluindo BOM/espaços. Depois do hash, ainda é obrigatório autorizar o peer público mostrado pelo convidado. Chaves privadas e a sessão são novas a cada execução.

O [exemplo](../apps/connect-lab/examples/host-downstream.json) usa somente loopback e portas de ensaio: transporte 42000, TCP 42001, UDP 42002, 600 segundos, downstream 100.000 bytes/s e 100 datagramas/s. São valores de laboratório, não configuração recomendada de mod. A CLI aceita `game`/TCP obrigatório e `aux`/UDP opcional, revisão 1; não é catálogo genérico, manifesto de mod ou novo contrato remoto. Acrescentar `aux` altera o hash e exige nova revisão. Plano vazio, somente `aux`, IDs repetidos/desconhecidos e destinos não loopback são recusados. Escolher uma porta de transporte concreta; zero não é aceito nesta CLI.

Com o endpoint `echo-downstream` rodando em outro terminal, revisar o plano:

```powershell
$lab = './apps/connect-lab/target/debug/aurora-connect-lab.exe'
& $lab echo-downstream 127.0.0.1:42001 127.0.0.1:42002 600
```

Em outro terminal, no diretório desta worktree:

```powershell
$lab = './apps/connect-lab/target/debug/aurora-connect-lab.exe'
$planFile = './apps/connect-lab/examples/host-downstream.json'
$review = (& $lab plan $planFile | ConvertFrom-Json).plan
$review | ConvertTo-Json -Depth 8
```

Após revisar destinos, orçamento, prazo e hash, iniciar e autorizar o convidado pelo mesmo procedimento de troca de pins abaixo:

```powershell
& $lab host-plan $planFile $review.hash
```

O convidado usa o comando `guest` existente. Para este endpoint sintético, executar `probe-downstream` em vez de `probe`. Encerrar por `stop`, Ctrl+C ou prazo. Estes comandos de plano não escrevem configurações; não existe snapshot de configuração pessoal a restaurar. Os adaptadores nativos opt-in de propriedades/TOML em cópias de diagnóstico têm fluxo próprio de autorização, snapshot/journal e recuperação; esse plano de serviços não os autoriza. O plano contém destinos para revisão local; não compartilhar esse JSON como log sanitizado.

## Ensaio em dois computadores — preparado, ainda não executado

Há também um [roteiro curto por computador](aurora-connect-lan.md), com os dois blocos do convidado, hash fixado e encerramento, preparado para o computador do irmão do usuário. A conversa só controla este PC; copiar/iniciar o convidado requer alguém naquele computador. Os testes locais independentes continuam. O kit é ferramenta privada de ensaio, sem instalação/publicação, e não leva Minecraft/mods/credenciais.

Impedimento observado: as ferramentas desta tarefa só têm acesso a este Windows; nenhum segundo host/rede autorizado está conectado. O teste local pode ser concluído aqui; LAN e redes distintas exigem o segundo endpoint. A ação humana mínima é disponibilizar um segundo Windows autorizado e copiar o executável debug preparado, sem instalar Rust ou programar. Somente participar do teste externo quando desejado; isto não constitui homologação de voz/jogo.

Arquivo produzido pelo build local: `apps/connect-lab/target/debug/aurora-connect-lab.exe`. Copiar esse arquivo para `C:/AuroraConnectLab/aurora-connect-lab.exe` no computador convidado por um meio privado; o host usa o executável já compilado. O hash SHA-256 local deve ser comparado após a cópia. Nenhum segredo ou mod acompanha o arquivo. É ferramenta debug experimental de teste, não instalador/release pública. Nos comandos abaixo `$lab` aponta para esse executável; portas são escolhas explícitas do ensaio, não defaults dos mods.

### Computador anfitrião

1. Em um terminal, iniciar endpoints controlados locais:

```powershell
$lab = 'C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect/apps/connect-lab/target/debug/aurora-connect-lab.exe'
& $lab echo 127.0.0.1:42001 127.0.0.1:42002 600
```

2. Em outro terminal, iniciar o agente. O bind abaixo expõe somente o transporte autenticado na porta UDP 42000; serviços ficam em loopback:

```powershell
$lab = 'C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect/apps/connect-lab/target/debug/aurora-connect-lab.exe'
& $lab host 0.0.0.0:42000 127.0.0.1:42001 127.0.0.1:42002 600
```

3. A linha `host_identity` fornece `peer` público e `session`; enviar os dois privadamente ao convidado, junto ao endereço IPv4 alcançável desse computador e porta `42000`. Não usar `0.0.0.0` como destino. Nenhuma porta é mapeada automaticamente. Não mudar regras de firewall/roteador nesta etapa. Se houver bloqueio, registrar a falha; timeout não identifica o tipo de NAT.
4. Receber `peer` público de `guest_identity`, colá-lo no terminal do host e pressionar Enter. Apenas após `host_ready`, avisar o convidado que pode digitar `ready`.

### Computador convidado

1. Em um terminal, receber os valores públicos do host e iniciar o agente:

```powershell
$lab = 'C:/AuroraConnectLab/aurora-connect-lab.exe'
$hostPeer = Read-Host 'Peer publico mostrado pelo anfitriao'
$hostAddress = Read-Host 'Endereco direto do anfitriao, IP:42000'
$session = Read-Host 'Session mostrada pelo anfitriao'
& $lab guest $hostPeer $hostAddress $session 127.0.0.1:42010 127.0.0.1:42011 600
```

2. Enviar o `peer` público mostrado em `guest_identity` ao host. Esperar a confirmação `host_ready`; então digitar `ready` no terminal do convidado. A linha `guest_ready` significa transporte/serviços do laboratório, não entrada no mundo ou voz validada.
3. Em outro terminal, executar o probe já preparado:

```powershell
& 'C:/AuroraConnectLab/aurora-connect-lab.exe' probe 127.0.0.1:42010 127.0.0.1:42011
```

Resultado esperado: `probe_passed` com 1.048.576 bytes TCP por direção e dez datagramas íntegros. Falha gera código sanitizado; não compartilhar linhas de identidade, endereço, chave privada, conta, áudio ou logs integrais.

### Encerramento e recuperação — ambos

Digitar `stop` no agente convidado e depois no host. No terminal echo, digitar `stop`. Ctrl+C e o prazo máximo de 600 segundos também encerram. Chaves efêmeras e leases deixam de existir, e sockets são fechados. Não há configuração pessoal, regra de firewall ou mapeamento a restaurar. Para repetir, iniciar novamente e trocar os novos pins/sessão; valores da execução antiga não dão acesso à nova.

Devolver somente: PASS/falha e código, versões/hash do binário iguais ou diferentes, sistema operacional de cada computador, LAN ou redes distintas, IPv4/IPv6 usado, duração e campos agregados de `probe_passed`. Para redes geográficas futuras, informar apenas países/regiões consentidos, sem IP. Isso desbloqueia evidência AUR-R4-004 do caminho efetivamente testado, não todas as topologias. IPv6 requer endereço `[IPv6]:42000` e bind host `[::]:42000`; o ensaio externo ainda precisa de firewall/alcance verificados sem alterações automáticas.

## Limites e próximos gates

Incremento 18: o mesmo smoke `connect-minecraft-status-smoke.ps1 -GamePort 43001 -SkipBuild` passou contra Minecraft 1.21.1/Fabric 0.16.14 real, com bind pertencente ao Java do laboratório. Foram observados protocolo 767, zero jogadores, resposta/pong, encerramento dos dois agentes e depois stop normal do servidor/EULA restaurada. Configuração própria `online-mode=false` solicitada pelo usuário e whitelist ativa; sem login, entrada no mundo, cliente original/offline ou voz homologados. [Executor, hashes e resultados](aurora-connect-runtime.md).

- Uma sessão host, até oito peers, destinos/serviços explícitos, lifetime máximo de uma hora. A CLI autoriza um convidado; testes da biblioteca exercitam vários. Não há convites sociais/contas/interface no produto.
- Payload UDP útil é o menor teto autorizado/negociado e `max_datagram_size − 24` atual. Limite pode diminuir; aumento depende da negociação da associação. Oversize é descartado integralmente e contado, nunca truncado ou fragmentado. Falta de DATAGRAM recusa serviço.
- A política padrão `ReplyBudget` limita o retorno UDP a 1:1 incluindo framing, sem crédito inicial e com teto de 64 KiB. `AuthorizedDownstream` exige plano explícito do host para o serviço UDP, com tetos de bytes/s e datagramas/s por participante/serviço. O retorno começa somente após o primeiro envio autorizado ao endpoint; quotas são compartilhadas entre associações e preservadas na reconexão. Expiração absoluta, revogação, MTU, socket conectado e inatividade por ingresso continuam obrigatórios. Dez testes reais adicionais cobrem essa política. Receber mais bytes não prova compatibilidade de voz nem endereçamento do mod.
- Filas de aplicação UDP têm até oito mensagens/40 ms, com descarte; buffers internos de transporte também são limitados, mas não há garantia de prazo de áudio em congestionamento. TCP sob carga e voz em redes reais precisam de medição própria.
- Listeners loopback não autenticam processos locais. Um processo com acesso à máquina pode tentar usar a porta; autenticação do jogo/mod continua obrigatória. Nenhum IP original é inventado para o serviço.
- NAT duplo/CGNAT, UDP externo bloqueado, relay/migração, perda real, BR–BR/BR–EUA, runtime Minecraft, SVC e Plasmo e UI permanecem gates separados. O inspector Node só produz plano/diff. Os [adaptadores nativos de configuração](aurora-connect-voice.md) alteram e desfazem anúncio somente em cópias de diagnóstico verificadas, sob feature opt-in/confirmada, com fixtures testadas; não há comando/UI conectado ao fluxo de jogo ou configuração pessoal alterada.

Não habilitar o laboratório no launcher nem ampliar a matriz a partir destes resultados. Rollback operacional é encerrar os agentes; rollback de código é reverter commits desta tarefa de forma revisável, sem reset ou apagar worktrees.
