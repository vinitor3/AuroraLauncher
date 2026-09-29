# Aurora Connect — laboratório local executável

Estado: **PROTÓTIPO**, fora do desktop e desativado para usuários. Base alpha.14, Core 1.1.5/Skins 0.3.1 preservados. [Execução/evidências](task-aur-r4-connect.md), [contrato canônico](engineering/AURORA_CONNECT_SPEC.md), [voz](aurora-connect-voice.md).

## O que este executável faz

Dois agentes autenticam as chaves públicas esperadas usando Iroh 1.3.0 e QUIC cifrado. O host concede uma lease de laboratório ao peer convidado, sessão e serviços explícitos, com prazo e revogação. O convidado pede apenas IDs `game` (TCP) e `aux` (UDP), revisão 1. Os destinos aprovados são literais loopback do host; o convidado não escolhe IP/porta do destino.

Cada TCP usa stream próprio, buffers limitados, backpressure e half-close. UDP usa DATAGRAM autenticado com cabeçalho de 24 bytes e socket host exclusivo por associação; o retorno fica vinculado à conexão autenticada, serviço, associação e geração. IDs não são autorização. O conteúdo é opaco; não há voz própria, captura de áudio ou bypass de autenticação Minecraft/mod.

Transporte é somente direto com endereço manual. Relay, descoberta pública, portmapper e sinalização em nuvem estão desligados. Não há VPN, driver, instalação do launcher, cobrança ou alteração de firewall/roteador. Loopback não demonstra LAN, NAT traversal ou internet. Pin público e endereço são dados de conexão a trocar privadamente entre participantes; nenhuma chave privada é impressa ou persistida.

## Reproduzir neste computador

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

## Plano explícito de serviços do laboratório

O comando convencional `host` mantém `ReplyBudget`. Para autorizar o perfil assimétrico, `plan` lê um JSON local de até 16 KiB, apresenta o estado antes/depois e seu SHA-256. `host-plan` exige esse hash, relê os bytes e recusa qualquer diferença antes de abrir transporte. A revisão é dos bytes exatos, incluindo BOM/espaços. Depois do hash, ainda é obrigatório autorizar o peer público mostrado pelo convidado. Chaves privadas e a sessão são novas a cada execução.

O [exemplo](../apps/connect-lab/examples/host-downstream.json) usa somente loopback e portas de ensaio: transporte 42000, TCP 42001, UDP 42002, 600 segundos, downstream 100.000 bytes/s e 100 datagramas/s. São valores de laboratório, não configuração recomendada de mod. Exatamente `game`/TCP e `aux`/UDP, revisão 1, são aceitos pela CLI atual; não é catálogo genérico, manifesto de mod ou novo contrato remoto. Campos desconhecidos e destinos não loopback são recusados. Escolher uma porta de transporte concreta; zero não é aceito nesta CLI.

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

O convidado usa o comando `guest` existente. Para este endpoint sintético, executar `probe-downstream` em vez de `probe`. Encerrar por `stop`, Ctrl+C ou prazo. Estes comandos de plano não escrevem configurações; não existe snapshot de configuração pessoal a restaurar. Um futuro adaptador que edite propriedades/TOML terá fluxo próprio de autorização, snapshot/journal e recuperação. O plano contém destinos para revisão local; não compartilhar esse JSON como log sanitizado.

## Ensaio em dois computadores — preparado, ainda não executado

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

- Uma sessão host, até oito peers, destinos/serviços explícitos, lifetime máximo de uma hora. A CLI autoriza um convidado; testes da biblioteca exercitam vários. Não há convites sociais/contas/interface no produto.
- Payload UDP útil é o menor teto autorizado/negociado e `max_datagram_size − 24` atual. Limite pode diminuir; aumento depende da negociação da associação. Oversize é descartado integralmente e contado, nunca truncado ou fragmentado. Falta de DATAGRAM recusa serviço.
- A política padrão `ReplyBudget` limita o retorno UDP a 1:1 incluindo framing, sem crédito inicial e com teto de 64 KiB. `AuthorizedDownstream` exige plano explícito do host para o serviço UDP, com tetos de bytes/s e datagramas/s por participante/serviço. O retorno começa somente após o primeiro envio autorizado ao endpoint; quotas são compartilhadas entre associações e preservadas na reconexão. Expiração absoluta, revogação, MTU, socket conectado e inatividade por ingresso continuam obrigatórios. Dez testes reais adicionais cobrem essa política. Receber mais bytes não prova compatibilidade de voz nem endereçamento do mod.
- Filas de aplicação UDP têm até oito mensagens/40 ms, com descarte; buffers internos de transporte também são limitados, mas não há garantia de prazo de áudio em congestionamento. TCP sob carga e voz em redes reais precisam de medição própria.
- Listeners loopback não autenticam processos locais. Um processo com acesso à máquina pode tentar usar a porta; autenticação do jogo/mod continua obrigatória. Nenhum IP original é inventado para o serviço.
- NAT duplo/CGNAT, UDP externo bloqueado, relay/migração, perda real, BR–BR/BR–EUA, runtime Minecraft, SVC e Plasmo e UI permanecem gates separados. O inspector Node só produz plano/diff. Os [adaptadores nativos de configuração](aurora-connect-voice.md) alteram e desfazem anúncio somente em cópias de diagnóstico verificadas, sob feature opt-in/confirmada, com fixtures testadas; não há comando/UI conectado ao fluxo de jogo ou configuração pessoal alterada.

Não habilitar o laboratório no launcher nem ampliar a matriz a partir destes resultados. Rollback operacional é encerrar os agentes; rollback de código é reverter commits desta tarefa de forma revisável, sem reset ou apagar worktrees.
