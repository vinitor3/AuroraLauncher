# Convites externos — retomada85

Data: 23/10/2026. Estado: **PROTÓTIPO**; direto externo **PARCIAL**, com uma nova amostra em outro computador/outra internet confirmada pelo usuário. Foram quatro tentativas separadas. Houve admissão e recusa de replay na primeira; a terceira acrescentou TCP sintético de1MiB por direção e revogação da conexão QUIC ainda ativa, depois do EOF do stream TCP. A tentativa de expiração120s não chegou à admissão e não comprova expiração de sessão ativa.

Este registro conserva as falhas e separa relato remoto de observação local. Não atribui estes resultados aos testes nativos de voz85D/E nem substitui o [roteiro47/47b](aurora-connect-internet-invitations.md), a [correção de entrada47c](aurora-connect-invite-input.md) ou o [contrato de observação54](aurora-connect-connection-diagnostics.md).

## Artefatos e preparação

O convidado usou o kit privado `connect-invite-kit-resume85-237ea20c.zip` e o alias explícito `guest-external-ipv6-interactive-observe`. Serviço aprovado: somente game/TCP sintético; QUIC usa transporte UDP, mas não houve teste de serviço UDP da aplicação. Relay permaneceu desligado. Preparação600s, Hello10s e TTL máximo600s conservaram os contratos anteriores; a quarta tentativa usou lease120s separada. Não houve renovação, aprovação ou reinício automáticos.

| Artefato efetivamente registrado | SHA-256 |
| --- | --- |
| ZIP privado do convidado | `2ed98260859a848d7fb5f5d50d5dd88eb6e22be591d52232a467a16b0cf3a091` |
| Executável convidado,23502336B | `237ea20c330458c9585dd2459542e7c8f373f446e67b404929bb8024af7932d0` |
| Executável anfitrião das quatro tentativas | `07f513a2123964c22c1a154b59c0ea9cce1982d8931e9a9ab5c6b8942967c172` |
| `guest-observed.ps1` | `561077cbebf86a5bfce445bca5334c2eaba0e2e51c28954d1e8cb665cf37f7a4` |
| `probe-tcp.ps1` | `d50c0262cc75ce66ffcf35291f2d94b8486caad1c926c91f58a8f56d27db803a` |

O manifesto do kit continua sendo o snapshot da criação: `testedTwoComputers=false` e host de referência47b `7d3403b2…b3e16`. Não foi reescrito para incorporar os resultados externos ou trocar o host de referência. A composição inicial47b/convidado novo passou localmente em2575ms; a composição com o host efetivamente usado passou separadamente em2020ms. Ambas transferiram1MiB por direção, mantiveram identidade no replay/revogação, observaram denied0→1→2 e cleanup/rebind/exit0 sem força. Depois da segunda tentativa externa, o mesmo probe PowerShell pinado atravessou o Guest real em loopback e passou em3960ms, com os mesmos controles. Esses três resultados são somente locais.

O anfitrião usou o caminho corrente coberto pela regra Private já existente; não houve alteração de firewall. Consulta de relógio somente leitura observou offset do anfitrião de aproximadamente-6,765s. Não houve ajuste de relógio, configuração pessoal ou roteador. Essa amostra não mede o relógio remoto nem estabelece a causa de uma falha de transporte ou de coordenação posterior.

## Origem das evidências

Os eventos do convidado, inclusive observações de fase, resultados do probe, fechamento e cleanup, foram enviados pelo usuário a partir do outro PC. As ferramentas do coordenador não observaram diretamente esse computador. Contadores, encerramento dos processos próprios do anfitrião/echo, códigos de saída e rebind foram observados localmente. A topologia de outro computador/outra internet foi confirmada pelo usuário; os relatórios privados preservam essa atribuição.

`elapsedMs` da observação54 é tempo monotônico da API, incluindo etapas locais e cleanup conforme o contrato. O tempo do probe inclui a operação completa. Nenhuma dessas medidas é RTT isolado da rede. `admitted/ok` comprova conclusão do handshake nativo; `audit_guest_ready` é a prontidão do runner. TCP, contadores do destino e cleanup têm provas adicionais próprias abaixo.

## Tentativa1 — admissão, replay e revogação após fechamento

Relato remoto: observação `admitted/ok` em489ms e prontidão. Antes de qualquer probe TCP observado, o convidado relatou fechamento antecipado com `deadlineReached=false`. A causa desse fechamento permanece **NÃO VERIFICADO**. O idle padrão120s do host foi identificado por leitura estática e permanece **HIPÓTESE**, sem correlação causal que permita atribuir-lhe esse fechamento.

O mesmo processo/identidade do convidado executou `retry-replay`: `hello_ack/replayed_request`,616ms, `passed=true`; o host passou de denied0 para1. Depois de revogar, `retry-revoked` retornou `hello_ack/expired_or_revoked`,630ms, `passed=true`, mesma identidade; denied1→2. Como a conexão original já havia fechado, esse caso prova a recusa da nova tentativa revogada, sem provar interrupção de conexão ativa.

Observação local final: zero destinos TCP/bytes; host exit0, motivo stop, revoked=true, activePeers0 e rebind. Echo aceitou zero conexões/bytes, saiu0 sem falha e passou cleanup/rebind. Relato remoto de stop: cleanup=true, dois retries e mesma identidade. Nenhuma expiração ativa foi testada.

Duas montagens locais do convite foram recusadas antes do envio; não houve envio de convite inválido. Elas não contam como tentativas remotas adicionais. A integridade da cópia enviada foi conferida antes do encaminhamento.

## Tentativa2 — admissão e probe recusado

Relato remoto: `admitted/ok` em479ms, prontidão, seguido de `kit_tcp_probe` com `passed=false`, `code=probe_failed`, zero bytes por direção,2131ms e cleanup=true. Não foi fornecido evento remoto de fechamento da conexão que identifique a fase do problema.

Observação local final: zero destinos TCP/bytes e denied0; echo sem conexão/bytes. Host e echo saíram0, sem força, com rebind; echo cleanup=true/failed=false. Relato remoto de stop: cleanup=true, zero retries, mesma identidade. A causa do probe recusado permanece **NÃO VERIFICADO**; idle120s é somente **HIPÓTESE** estática. O PASS local posterior do mesmo helper não diagnostica retrospectivamente a falha remota nem a converte em PASS.

## Tentativa3 — TCP completo e revogação de QUIC após EOF

Relato remoto: `admitted/ok` em560ms, prontidão e probe aprovado em5724ms, cleanup=true,1048576B enviados e1048576B recebidos e comparados ao payload original. Observação local: um destino TCP e2097152B cumulativos no host; echo aceitou uma conexão, recebeu1048576B e escreveu1048576B, sem falha. Assim, o TCP sintético de1MiB por direção passou nesta tentativa externa.

Antes de revogar, o host tinha activePeers1, activeTcp0, denied0 e revoked=false: o stream TCP já terminara por EOF, enquanto a conexão QUIC permanecia ativa. A revogação retirou essa conexão; o convidado relatou fechamento/rebind com `deadlineReached=false` e mesma identidade. Não houve revogação durante payload TCP em curso.

O retry após revogação retornou, remotamente, `hello_ack/expired_or_revoked`,570ms, `passed=true`, mesma identidade. Localmente, denied0→1, activePeers0 e revoked=true; o total permaneceu em um destino/2097152B. Esse caso não executou novo teste de replay; o replay aprovado pertence à tentativa1.

Relato remoto de stop: cleanup=true, um retry e mesma identidade. Host e echo saíram0, sem força, com rebind; echo cleanup=true. Não houve teste de expiração ativa nesta tentativa.

## Tentativa4 — lease120s vencida antes da admissão

Foi preparado convite novo com integridade conferida e lease120s para o ensaio separado de expiração. A janela de coordenação transcorreu sem admissão. O usuário relatou `audit_failed`, código `expired_or_revoked`, seguido de `kit_guest_finished`; não houve `audit_guest_ready` nem sessão ativa observada.

Localmente, o host encerrou por `lease_expired`, exit0/rebind, zero admissões, destinos TCP, bytes e recusas. Echo sem conexão/bytes, exit0, failed=false, cleanup/rebind; sem encerramento forçado. O probe TCP não foi executado.

O resultado é um negativo de validade antes da admissão. O código remoto agrupa expiração/revogação e não constitui prova isolada da causa temporal; o fechamento local por lease e a coordenação sem admissão delimitam o resultado observado. **Expiração de sessão ativa continua PRECISA DE TESTE**. Não há correlação de prontidão anterior ao deadline e fechamento posterior da mesma sessão para satisfazer esse aceite.

## Resultado permitido e limites

| Recorte | Resultado desta rodada |
| --- | --- |
| Convite externo assinado, revisão/aprovação e admissão | PASS nas tentativas1–3; **PROTÓTIPO** |
| Replay da mesma identidade | PASS na tentativa1, com recusa remota e contador host0→1 |
| Recusa de nova conexão após revogação | PASS nas tentativas1 e3, com contadores correlacionados |
| TCP sintético1MiB por direção | PASS na tentativa3, corroborado por host e echo |
| Revogação da conexão QUIC ainda ativa | PASS na tentativa3, após EOF TCP; interrupção durante payload não testada |
| Lease120s antes da admissão | Negativo observado na tentativa4; não aprova expiração ativa |
| Expiração de sessão ativa | **PRECISA DE TESTE** |

As quatro tentativas tiveram host/echo coletados com exit0/rebind e sem força; as provas de stop/cleanup do convidado permanecem relatos remotos. Não se atribui código de saída OS remoto a eventos de cleanup ou a `kit_guest_finished`.

Direto externo permanece **PARCIAL**: esta amostra não mede caminhos/migração, NAT/CGNAT, saturação, estabilidade prolongada ou outras topologias. Não houve relay, UDP da aplicação, Minecraft, login/mundo, dois jogadores, Core/Skins, interface Tauri, SVC/Plasmo, autenticação do mod ou áudio nesta rodada. Firewall, relógio e configurações pessoais permaneceram sem alteração. Falhas históricas47/47b/47c e as tentativas1/2 desta rodada são preservadas; os PASS posteriores não as reclassificam.

## Rastreabilidade privada e encerramento

Os quatro relatórios abaixo ficam sob `.aurora/connect-validation/`, ignorados pelo Git. Contêm campos redigidos, com atribuição de fonte; não são autoridade para repetir conexão, recuperar identidade ou reutilizar convite. Os hashes foram conferidos por leitura após seus fechamentos.

| Tentativa | Caminho relativo de `closed.json` | SHA-256 |
| --- | --- | --- |
|1 | `external-invites-resume85-703529059b9740fabf5f8d6a8e6aee9e/closed.json` | `62b460d961d19a81db9646f91ec475b10e750f371b87c950fd7fcc1a528f4165` |
|2 | `external-invites-tcp85-653e39c1c7b543c7ab91fc178ba79f45/closed.json` | `3638ae4c40d20b3e284a8e6042203d992f770b33da133159ccf011b8f232e85f` |
|3 | `external-invites-tcp85-immediate-d2f331b1c46c45eb9ebec4b5414ddee9/closed.json` | `75fda9b3c40587cd1c20c210e862df6df6aea8d880cd8158b0305e430adfe8d1` |
|4 | `external-invites-expiry85-885dc56516f346bb887ff3dca3aa1df8/closed.json` | `8d020365f566d36fd60f5eedc18319e64cf58568b4dc12ec1b26f254f8e8c481` |

Logs locais separados: `connection-resume85-mixed.log`, `connection-resume85-mixed-host.log` e `connection-resume85-powershell.log`. Manifesto/ZIP do kit continuam imutáveis, com os resultados externos em registros novos. Este documento não contém IP, peer, convite, assinatura, chave privada ou payload reais.

O encerramento operacional foi concluído nos quatro casos. Em nova execução, usar processos/identidades/plano/convite novos, conferência de integridade e aprovação explícita; conservar o kit e as evidências anteriores. Antes da prontidão, Ctrl+C é cancelamento; depois, preferir stop e aguardar cleanup/rebind. Se o programa terminou e o shell reapareceu, não colar convite ou aprovação no shell. Não há configuração pessoal a restaurar por esta rodada.
