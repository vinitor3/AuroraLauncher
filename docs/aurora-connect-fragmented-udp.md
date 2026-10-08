# Aurora Connect — UDP fragmentado experimental

Data: 08/10/2026. Incremento 49. Estado: **PROTÓTIPO**, com transporte real validado em loopback e suíte integrada final aprovada. Áudio e V2 entre redes continuam sem validação. Este documento detalha a implementação de laboratório e complementa a [especificação canônica](engineering/AURORA_CONNECT_SPEC.md), o [domínio Connect](aurora-connect.md) e o [domínio de voz](aurora-connect-voice.md). Não altera as prioridades do roadmap nem homologa Minecraft, áudio, interfaces ou os nove pares Core/Skins.

A implementação começou sobre `0e7eaa344a5f59556880b56b53db7d6bfd31fdab` (integração desktop 46). No registro deste documento, o HEAD é `2b18e69d8df63bd07a0a769fd549f403af4b9b46`, que acrescenta o runner de convites 47. As mudanças 49 ainda não constituem um commit ou kit publicado. O kit 47 histórico preserva seu binário congelado. A suíte final também inclui a atualização 47b do runner, com preparação interativa explícita; ela não habilita UDP V2 nos convites.

## Escopo e autorização

O modo novo transporta bytes UDP opacos, inclusive um datagrama vazio. Não codifica áudio, não interpreta pacotes de mods e não modifica configuração de jogo, MTU do mod, Java, firewall ou relay. Os JARs e pins SVC 2.6.22/Plasmo 2.1.17 permanecem preservados. Os tamanhos de 1427 e 2108 B usados nos testes são motivados pela análise estática dos protocolos; os dados transportados são padrões sintéticos, não pacotes de áudio produzidos pelos mods.

O transporte autenticado, o ALPN `aurora/connect-lab/1` e o schema 1 do controle permanecem. A versão 2 se aplica somente ao envelope UDP explicitamente selecionado. Não existe fallback automático de V1 para V2. Sucesso em `send()` significa enfileiramento local dos frames, sem comprovar entrega remota: a perda de um fragmento pode descartar o datagrama inteiro. Não há retransmissão ou garantia de entrega ordenada.

O host deve autorizar um `Service` UDP com `required_capability = Capability::FragmentedDatagram` e `max_datagram_bytes <= 4096`. O endpoint continua sendo um endereço literal loopback, com porta não zero e revisão válida, definido pelo host. O participante precisa estar autorizado pela lease para aquele serviço. Declarar uma capability no Hello não concede um serviço.

O Hello pode incluir a nova capability `fragmentedDatagram`; o limite de entradas passa a três para acomodar também `reliableStream` e `datagram`. A abertura usa a nova operação `openFragmented`. A combinação serviço UDP comum/abertura fragmentada e a combinação serviço fragmentado/abertura comum são recusadas. Serviço ausente, revisão divergente ou capability ausente são recusados antes de abrir o socket de destino.

Somente uma abertura V2 bem-sucedida acrescenta `fragmentVersion: 2` à resposta. Nesse caso, `maxPayload` significa o teto do **datagrama original remontado**, limitado a 4096 B. No V1, o campo opcional não é emitido e `maxPayload` conserva seu significado anterior de payload de um único frame.

Os validadores atuais de plano CLI e de convite continuam restringindo seus grants aos tipos anteriores. Testes verificam que `fragmentedDatagram` não passa por esses validadores. Este incremento não cria CLI, convite assinado, comando Tauri ou seleção de voz para V2; o opt-in está na API de biblioteca e nos testes. Os callers desktop 46 e runner 47 continuam com seus contratos anteriores.

## APIs e arquivos

| API | Contrato |
| --- | --- |
| `Guest::open_udp_fragmented(service, revision)` | Retorna `FragmentedUdpAssociation` após autorização e confirmação V2 |
| `FragmentedUdpAssociation::send(&[u8])` | Envia um datagrama original, dividido em frames; erro pode abandonar uma mensagem parcialmente emitida |
| `FragmentedUdpAssociation::recv()` | Retorna `ReassembledDatagram` completo; nunca retorna um prefixo |
| `ReassembledDatagram::as_ref()` | Empréstimo dos bytes; o objeto conserva a reserva de memória até ser descartado |
| `association_id`, `service_handle`, `generation`, `max_payload`, `is_closed`, `shutdown` | Consultas e encerramento próprios da associação |
| `Guest::udp_bridge_fragmented(service, revision, bind)` | Retorna `UdpBridge` com listener loopback e associação independente por origem local |
| `MAX_FRAGMENT_FRAME_BYTES` | Constante pública de 1264 B, incluindo o header V2 |

`open_udp`, `udp_bridge`, `UdpAssociation::recv() -> Bytes`, TCP e o codec V1 mantêm suas APIs. Não há conversão pública que separe os bytes de um `ReassembledDatagram` da sua reserva. O caller pode manter o objeto durante um `socket.send(...).await`; a ponte integrada faz isso.

Arquivos do incremento, relativos à raiz da worktree:

- `apps/connect-lab/src/fragment.rs`: codec, sequência, remontagem, reservas e testes puros.
- `apps/connect-lab/src/bridge.rs`: pools processuais por identidade e teto dinâmico de envio.
- `apps/connect-lab/src/model.rs`, `wire.rs`, `lib.rs`: capability, operação, resposta e exports.
- `apps/connect-lab/src/host.rs`, `guest.rs`: autorização, transporte, retorno, fila e encerramento.
- `apps/connect-lab/tests/fragmented_transport.rs`: agentes QUIC e endpoints TCP/UDP reais em loopback.

Nenhuma dependência ou Cargo manifest/lock foi alterado por este incremento. Alterações simultâneas do desktop e de descoberta pertencem a outros incrementos.

## Envelope de 32 bytes

Todos os inteiros usam big-endian. O tamanho declarado do fragmento precisa corresponder exatamente aos bytes restantes; truncamento ou bytes extras são recusados.

| Offset | Tamanho | Campo |
| --- | ---: | --- |
| 0 | 1 B | Versão, exatamente 2 |
| 1 | 1 B | Flags, exatamente 0 |
| 2–5 | 4 B | Handle do serviço autorizado |
| 6–13 | 8 B | ID da associação |
| 14–17 | 4 B | Geração/revisão do serviço |
| 18–25 | 8 B | ID da mensagem, de 1 a `u64::MAX` |
| 26–27 | 2 B | Tamanho total do datagrama original, de 0 a 4096 |
| 28 | 1 B | Índice do fragmento, começando em 0 |
| 29 | 1 B | Quantidade de fragmentos, de 1 a 8 |
| 30–31 | 2 B | Tamanho deste fragmento |
| 32 em diante | Variável | Bytes opacos do fragmento |

Peer e sessão não são campos declarados pelo remetente nesse envelope: vêm da conexão autenticada e de sua associação autorizada. Uma instância de remontagem pertence a uma conexão/sessão, serviço, associação e geração; não é compartilhada entre essas identidades. Frames V2 não entram em associações V1, nem frames V1 em associações V2.

Para um datagrama com `total > 0`, o emissor calcula `count = ceil(total / capacity)` e `K = ceil(total / count)`. Os primeiros `count - 1` fragmentos têm K bytes; o último contém o restante, estritamente positivo. O índice determina `offset = index * K`, sem campo de offset livre. O receptor valida esse formato antes de reservar memória, impedindo sobreposição, fragmento vazio intermediário e comprimento inconsistente. O datagrama vazio usa exclusivamente `total=0`, `count=1`, `index=0` e payload vazio.

Cada direção da associação tem uma sequência independente. O emissor começa em 1, não reutiliza IDs e recusa novo envio após `u64::MAX`; não existe wrap. O receptor conserva uma janela de 64 IDs, permitindo ordem invertida dentro dela. O ID fica marcado ao iniciar uma mensagem válida, inclusive se depois faltar quota ou a mensagem for descartada. Conclusão, timeout e inconsistência não liberam esse ID para uma segunda entrega. Avançar a janela descarta pendências que ficaram obsoletas. `clear()` libera as pendências sem reinicializar a janela; um novo estado requer nova associação/contexto autorizado.

Duplicata idêntica de um fragmento pendente não altera o prazo. Conteúdo diferente para o mesmo fragmento, total diferente ou quantidade diferente descartam a mensagem pendente. Um frame estruturalmente inválido é recusado antes da remontagem; não é tratado como confirmação ou novo fragmento válido.

## Capacidade, prazo e memória

O teto por frame completo é `min(connection.max_datagram_size(), 1264)`. A capacidade para bytes de fragmento é esse resultado menos 32. Os 1264 B são uma escolha conservadora do contrato V2, não medição de MTU física, e são distintos do payload V1 de 1264 B observado em outro ensaio.

O teto efetivo do datagrama original considera a capacidade atual vezes oito, o limite do serviço, o teto inicialmente aprovado na abertura e 4096 B. Antes de cada envio, o emissor consulta novamente a capacidade atual. Se ela ficar abaixo da capacidade usada para escolher a divisão daquela mensagem, abandona o restante. Não adapta um fragmento intermediário, não retransmite e não reutiliza o ID. Uma associação não ganha um teto original maior por crescimento posterior da capacidade; reabrir exige nova negociação.

Quando o frame completo pode usar 1264 B, cada fragmento pode carregar até 1232 B. Os testes reais observaram pelo menos nove frames QUIC DATAGRAM por direção e por peer para quatro mensagens de 0/1427/2108/4096 B: respectivamente 1/2/2/4 fragmentos. A ordem dos frames não é uma promessa de entrega do UDP.

O prazo de remontagem é 250 ms desde o primeiro fragmento aceito, sem renovação por atividade ou duplicatas. A recepção verifica o prazo antes de continuar; um timer local de 25 ms também libera pendências sem tráfego novo. O prazo lógico não é garantia de escalonamento em tempo real. Mensagens completas aguardam o consumidor na fila existente de até oito entradas e 40 ms; o convidado também limpa essa fila periodicamente para liberar guards mesmo quando ninguém chama `recv()`.

| Reserva de payload | Limite |
| --- | ---: |
| Por datagrama original | 4096 B |
| Por associação, incluindo mensagens completas ainda retidas | Oito mensagens e 32 KiB |
| Por identidade de peer no processo | 64 KiB |
| Global no processo | 1 MiB |

As reservas compartilhadas são adquiridas antes da alocação do payload. Falha em uma etapa libera as etapas anteriores. O guard continua vivo quando a remontagem sai da tabela e entra na fila, quando `recv()` a entrega ao caller e durante o envio local aguardado. Drop libera a reserva; `clear()` não libera antecipadamente um datagrama completo que outro caller ainda conserva.

O registro processual usa a identidade autenticada como chave, com referências fracas para não acumular peers inativos. Um guard completo ainda retido mantém vivo o contador daquela identidade; uma reconexão não obtém um pool novo enquanto esse contador existir. Esses limites contabilizam payload reservado e quantidade de mensagens, não RSS ou todos os metadados do alocador. Também continuam necessários os limites existentes de peers, associações, filas e buffers do transporte.

## Rate limit, retorno e encerramento

Cada fragmento V2 é cobrado como um datagrama e pelo tamanho completo, incluindo os 32 B de header. Recepção V2 ou de versão desconhecida cobra os limites da conexão antes de interpretar o envelope, inclusive para frames posteriormente recusados por formato, replay ou associação. A associação autorizada aplica também seu limite de serviço; o orçamento agregado permanece compartilhado com TCP. Defaults existentes: 200 datagramas/s, burst 40, 250000 B/s para UDP, burst de bytes 65536 e 2500000 B/s agregado. Esses valores são limites de configuração, não vazão medida.

No modo `ReplyBudget`, somente uma mensagem completa que foi encaminhada ao UDP local gera crédito de retorno, contado pelos bytes originais e headers dos fragmentos válidos dessa mensagem. Antes de emitir um retorno, o host desconta o custo integral previsto; depois, cada frame passa pelos limites aplicáveis. Falha de envio não devolve crédito para uma amplificação posterior. Variações entre as capacidades das duas direções podem fazer o retorno exigir mais headers e ser recusado por falta de crédito.

`AuthorizedDownstream` permanece uma política explicitamente configurada pelo host, com limites próprios por peer/serviço, dentro dos limites globais. Permite retorno maior que a solicitação somente após o primeiro encaminhamento válido ao endpoint. Não permite trocar o destino UDP por endereço anunciado no frame. O teste real aprovou 2108 B de retorno após uma solicitação curta nesse modo e recusou a mesma expansão em `ReplyBudget`; 4097 B no retorno foram recusados sem prefixo entregue ao convidado.

Retorno não renova o idle da associação nem a lease. O host conserva e verifica o token original da lease, seu prazo Unix e o prazo monotônico derivado da admissão, antes da remontagem e do I/O. Não depende apenas de uma tarefa watcher propagar cancelamento. A autorização por serviço/revisão precede qualquer alocação de remontagem e qualquer encaminhamento ao destino; a leitura estrutural do header é emprestada e não aloca payload.

O encerramento do convidado publica o cancelamento antes de limpar fila/remontagem. A recepção revalida cancelamento dentro dos locks relevantes, evitando recriar uma reserva após a limpeza. A ponte encerra as associações, aguarda suas tarefas e libera listeners; revogação ou fim da conexão também aciona cleanup. O teste específico mantém o handle fechado vivo e exige reserva zero, sem depender de seu Drop para aparentar sucesso.

`MetricsSnapshot` não ganhou campos. No caminho V2, `udp_forwarded`/`udp_returned` contabilizam datagramas lógicos encaminhados/enfileirados; não substituem contadores de fragmentos físicos. `udp_dropped` e `udp_oversize` continuam diagnósticos limitados, não um balanço completo de perda de rede ou toda expiração. A comprovação de fragmentação no teste usa `Connection::stats().frame_tx/frame_rx.datagram`, separadamente dos contadores do endpoint.

## Evidências executadas

Diretório utilizado: `C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect`. Os logs abaixo ficam em `.aurora/connect-validation/`, ignorado pelo Git. Nenhuma execução desta seção usou Java, mods, áudio, rede externa, relay ou configuração pessoal.

| Execução | Resultado e registro |
| --- | --- |
| Codec isolado, antes de registrar o módulo | 17 PASS, zero ignorados, 0,01 s; `connect-fragment-codec-tests-1.log` |
| Primeiros testes reais V2 | Seis PASS, zero ignorados, 2,20 s; `connect-fragment-transport-tests-1.log` |
| Biblioteca e transporte após correções iniciais | 29 testes da biblioteca PASS em 1,60 s e oito de transporte PASS em 3,06 s, zero ignorados; `connect-fragment-focused-2.log` |
| Suíte completa nesse snapshot | 144 PASS, três opt-ins ignorados; `connect-fragment-full-1.log` |
| Regressões finais após revisão | 19 PASS, zero ignorados, 11 filtrados, 0,11 s; `connect-fragment-qa-regressions-1.log` |
| Clippy após os oráculos de revisão | PASS com `-D warnings`; `connect-fragment-clippy-2.log` |
| Formatação e verificação de whitespace | PASS nos arquivos próprios; saída das ferramentas, sem log dedicado |
| Suíte integrada final, incluindo runner 47b | 147 PASS, zero falhas, quatro opt-ins ignorados; `connect-fragment-full-final-2.log` |
| Formatação final da crate | PASS; `connect-fragment-fmt-final-3.log` |
| Clippy final de todos os targets | PASS com `-D warnings`; `connect-fragment-clippy-final-3.log` |
| Build final dos dois binários | PASS; `connect-fragment-build-final-2.log` |
| Gate separado 47b com IPv6 próprio | Um PASS, zero ignorados, oito filtrados, 62,15 s; connect-invite-forty-seven-b-own-ipv6.log |

Os 19 testes focados incluem os 17 do codec, a regressão de cleanup fortalecida e um teste novo da autoridade original. O primeiro snapshot teve 144 PASS; após liberar os builds e incorporar também dois testes unitários do runner 47b, a execução unificada final teve **147 PASS, zero falhas e quatro opt-ins ignorados**. Não se deve apresentar a soma de rodadas focadas como uma suíte de 145 testes executada. Os quatro ignorados exigem endereço próprio explícito: IPv6 de convite, IPv6 do runner com dois processos, IPv6 do runner com preparação interativa de 61 segundos e IPv4 próprio. Ignorado não significa aprovado.

Depois da suíte padrão, o opt-in interativo 47b foi executado separadamente com endereço do próprio computador previamente revalidado, fornecido apenas pela variável temporária `AURORA_CONNECT_OWN_EXTERNAL_IPV6_TEST_ADDR` e restaurado em `finally`. Os dois processos originais aguardaram 61 segundos, trocaram 1 MiB TCP por direção, recusaram as duas tentativas negativas e concluíram cleanup. O log contém somente dados redigidos. Isso valida a preparação prolongada do runner no mesmo computador; não é evidência de UDP V2 ou comunicação entre duas redes.
Comandos efetivamente executados, sem os redirecionamentos de log:

```powershell
rustfmt --edition 2024 apps/connect-lab/src/fragment.rs
rustc --edition 2024 --test -F unsafe_code -D warnings apps/connect-lab/src/fragment.rs -o .aurora/connect-validation/connect-fragment-codec-tests.exe
& ./.aurora/connect-validation/connect-fragment-codec-tests.exe --test-threads=1
cargo check --offline --locked --manifest-path apps/connect-lab/Cargo.toml --all-targets
cargo test --offline --locked --manifest-path apps/connect-lab/Cargo.toml --test fragmented_transport --no-run
cargo test --offline --locked --manifest-path apps/connect-lab/Cargo.toml --test fragmented_transport -- --test-threads=1
cargo test --offline --locked --manifest-path apps/connect-lab/Cargo.toml --lib --test fragmented_transport -- --test-threads=1
cargo test --offline --locked --manifest-path apps/connect-lab/Cargo.toml -- --test-threads=1
cargo test --offline --locked --manifest-path apps/connect-lab/Cargo.toml --lib fragment -- --test-threads=1
cargo clippy --offline --locked --manifest-path apps/connect-lab/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path apps/connect-lab/Cargo.toml --all -- --check
cargo build --offline --locked --manifest-path apps/connect-lab/Cargo.toml --bins
cargo test --offline --locked --manifest-path apps/connect-lab/Cargo.toml --test invite_audit_cli own_ipv6_interactive_two_original_processes_wait_past_sixty_seconds -- --ignored --nocapture --test-threads=1
rustfmt --edition 2024 --check apps/connect-lab/src/lib.rs apps/connect-lab/src/model.rs apps/connect-lab/src/wire.rs apps/connect-lab/src/host.rs apps/connect-lab/src/guest.rs apps/connect-lab/src/bridge.rs apps/connect-lab/src/fragment.rs apps/connect-lab/tests/fragmented_transport.rs
git diff --check -- apps/connect-lab/src/lib.rs apps/connect-lab/src/model.rs apps/connect-lab/src/wire.rs apps/connect-lab/src/host.rs apps/connect-lab/src/guest.rs
git diff --no-index --check -- /dev/null apps/connect-lab/src/fragment.rs
```

O comando `rustc` isolado pertence ao snapshot inicial std-only; depois da integração, usar os testes Cargo correspondentes. A primeira compilação isolada falhou por dois erros nos testes: tipo inteiro ambíguo e consumo de `JoinHandle` dentro de `filter`. Ambos foram corrigidos; `connect-fragment-codec-compile-1.log` e `connect-fragment-codec-compile-2.log` conservam falha e sucesso. O primeiro Clippy apontou um módulo de testes antes de itens de produção; o módulo foi movido para o fim, sem silenciar lint. `connect-fragment-clippy-1.log` conserva essa falha.

A revisão independente encontrou e teve corrigidas a ordem limpar→cancelar no convidado e a dependência do watcher para perceber revogação/prazo no host. Os oráculos finais comprovam a ordem de cancelamento com o lock de remontagem retido, o token original cancelado enquanto o derivado permanece intacto e cada relógio de expiração separadamente. A revisão final não encontrou bloqueadores e não executou testes em paralelo.

Os artefatos debug reconstruídos ao final foram `aurora-connect-lab.exe`, 24.050.688 B, SHA-256 `005b55c43f2b08fd9bb5f32f7c17a81de1eccd50a34e920b95a545c580c5f973`, e `aurora-connect-invite-audit.exe`, 23.326.720 B, SHA-256 `7d3403b25ee66e1759adf87fe7d64bec65031d9574cc325b9ed64f09408b3e16`. O segundo inclui o ajuste 47b. Esses hashes identificam o build local revisado; não substituem pins ou kits anteriores nem indicam publicação.

## Alcance das provas e trabalho pendente

| Item | Evidência desta etapa | Limite |
| --- | --- | --- |
| Codec/remontagem | Testes puros de layout, tamanhos, ordem, repetição, inconsistência, quotas, liberação e IDs sem wrap | Não comprovam rede ou áudio |
| Identidade e memória | Teste puro conserva quota com guard retido após queda do owner e novo acesso ao pool da mesma identidade | Não é ensaio de exaustão do sistema operacional |
| Redução de capacidade | Teste puro recusa capacidade menor/ausente e preserva o layout inicial | Nenhuma redução real de PMTU foi induzida |
| TCP e UDP V2 | Agentes QUIC reais, dois peers/serviços, datagramas 0/1427/2108/4096 B nos dois sentidos e TCP 1 MiB por direção concorrente | Somente loopback; amostra curta, sem benchmark ou estabilidade prolongada |
| Autorização | Recusas de capability, modo, serviço, revisão e validações legadas; V1 continua transportando bytes | Não habilita V2 em convites ou desktop |
| Isolamento e descarte | Ordem invertida, duplicata, perda/expiração, campos de escopo incorretos, teto 4097 B e limitação por fragmento com endpoint real | Não caracteriza perda de uma rede externa |
| Retorno | ReplyBudget e AuthorizedDownstream separados, com recusa de expansão não autorizada e de retorno excessivo | Não comprova padrões/keepalive de nenhum mod |
| Encerramento | Revogação real, duas origens locais, rebind de listeners/origens/host/endpoints e regressões internas de autorização/cleanup | Não redefine o comportamento de processos ou configurações pessoais |

Continuam **PRECISA DE TESTE**: V2 em outra máquina/rede; áudio real de SVC e de Plasmo separadamente; endereçamento efetivo dos participantes; contas mistas, cliente anfitrião local, LAN e estabilidade. Nenhuma dessas lacunas deve ser substituída por compilação ou pelos bytes opacos desta suíte. Transporte V2 não resolve sozinho o anúncio de endereço do mod; as alternativas e restrições do [domínio de voz](aurora-connect-voice.md) continuam válidas.

Rollback operacional: não selecionar a capability/operação V2 e continuar usando as APIs V1 preservadas. Para encerrar um ensaio V2, aguardar `shutdown()` das pontes/associações e agentes e descartar datagramas completos ainda retidos pelo caller. Não há arquivo de configuração aplicado para restaurar. Reversão de código deve usar um commit revisado e isolado deste incremento, depois de criado, sem reset de outras mudanças da worktree. Pins, kits e manifestos históricos não são substituídos automaticamente por um debug novo.
