# Convites assinados entre duas redes — ensaios 47 e 47b

Atualização: 08/10/2026. Estado: **PROTÓTIPO**. O runner `aurora-connect-invite-audit` usa convites e transporte reais, mantendo a mesma identidade do convidado em RAM durante desconexão e tentativas negativas. Os ensaios automatizados de loopback e IPv6 próprio passaram; a validação do convite em dois computadores/outra internet continua **PRECISA DE TESTE**. A prova gráfica 44 usou o fluxo manual anterior e não aprova este convite.

## Histórico preservado e correção 47b

O kit 47, com runner `06726eec…6582`, continua imutável. Seus quatro comandos originais (`host`, `guest`, `host-external-ipv6`, `guest-external-ipv6`) usam preparação de 60s; o roteiro inicial tinha TTL de 300s para replay/revogação e 120s para expiração.

Na tentativa externa de 08/10, o convidado relatou erros de análise do PowerShell ao colar o convite depois de o processo nativo ter retornado ao shell, sem essa saída ter sido observada. Não foi obtida a linha original nem o código de saída nativo: a causa permanece **NÃO VERIFICADO**, sem atribuição a rede, identidade ou timeout. No anfitrião, houve zero destinos TCP e zero bytes; o host encerrou com `lease_expired`, exit0 e rebind. O echo não recebeu conexão e saiu0, com cleanup/rebind; as portas locais ficaram livres. Esse resultado não aprova entrada, replay, revogação ou expiração ativa.

O incremento 47b acrescenta somente os aliases explícitos `host-external-ipv6-interactive` e `guest-external-ipv6-interactive`, com 600s para cada prompt de preparação. Aprovação fica limitada ao menor prazo entre essa janela e a validade restante do convite. Hello/autenticação continuam em 10s; o TTL máximo da lease continua em 600s e não é renovado. Os comandos antigos não mudam de perfil.

Os eventos de identidade e revisão informam `preparationTimeoutSeconds` e `inputExpiresAtMs`. O runner revalida o prazo antes e depois de esperar entrada; uma linha enfileirada não vence um prazo já expirado. Isso não intercepta texto colado no PowerShell depois que o processo terminou. O launcher 47b avisa antes de iniciar e emite `kit_guest_finished` ao retornar.

### Falha de cópia do coordenador no ensaio 47b

Na primeira tentativa externa com o kit 47b, o convidado recebeu `invalid_protocol` porque o coordenador encaminhou uma cópia corrompida do convite. A saída ConPTY continha movimento de cursor na coluna 80 e sobrescrita de caracteres; remover somente os códigos ANSI, sem aplicar a sobrescrita, repetiu dois caracteres da assinatura. A cópia enviada tinha 130 caracteres hexadecimais nesse campo, em vez dos 128 exigidos. A reconstrução respeitando os movimentos do terminal recuperou o convite original, cuja assinatura e digest passaram na verificação local. A causa desta tentativa foi identificada na cópia feita pelo coordenador; não foi atribuída à rede nem à ação do convidado. Ela é distinta da tentativa histórica do kit 47, cuja causa de saída nativa permanece desconhecida.

O anfitrião registrou zero participantes admitidos, zero destinos TCP e zero bytes. Host e echo encerraram com código de saída 0 e rebind confirmado; o resultado não aprova entrada ou os ensaios negativos. Antes de encaminhar outro convite, o coordenador passou a conferir localmente a representação canônica, a assinatura Ed25519 e o digest SHA-256 da cópia exata que será enviada. Remover ANSI isoladamente não satisfaz esse gate. A conferência não emite convite novo, não altera seus prazos e não substitui a verificação independente do runner convidado. Convite, assinatura e identidades reais permanecem fora desta documentação e do Git.

## Preparação

O coordenador prepara e testa um kit privado novo, com executáveis fixados por SHA-256, procedimento e helpers TCP. O manifesto registra seu estado de criação, `testedTwoComputers=false`; resultados remotos posteriores ficam separados. Não substituir kit38, supervisor43 ou kit47. Não há instalador, Java, mod, mudança de firewall/roteador/configuração pessoal, relay ou custo. O convidado copia o kit para uma pasta local regular e confere o hash fornecido antes de executar. As ferramentas do coordenador alcançam apenas o anfitrião.

Kit atual: `.aurora/connect-validation/connect-invite-kit-47b-7d3403b2.zip`, 15.797.985B, sete entradas verificadas. Artefatos privados; não são release ou instalador.

| Artefato | SHA-256 |
| --- | --- |
| ZIP 47b | `d84ab12742f4a2b1125975b6f4ac2852567582d318e96455afd833c5bf0dbe1e` |
| `aurora-connect-invite-audit.exe` | `7d3403b25ee66e1759adf87fe7d64bec65031d9574cc325b9ed64f09408b3e16` |
| `aurora-connect-lab.exe` | `005b55c43f2b08fd9bb5f32f7c17a81de1eccd50a34e920b95a545c580c5f973` |
| `manifest.json` | `db6cf5d80ef58ae55a54dd6a739f2a76470c056286b80789b28feff81d714f7e` |

O anfitrião revalida seu endereço IPv6 e cria plano/hash novo autorizando somente `game/tcp`, destino `127.0.0.1:43001` e TTL de **600s para replay/revogação**. O echo permite uma conexão, 1MiB por direção e encerramento explícito. Seus bytes e os do probe permanecem iguais aos do kit47. Iniciar o echo **depois de receber a identidade do convidado**, antes de fornecê-la ao host/aprovar a revisão: assim a espera inicial não consome seu prazo de 720s. A tentativa separada de expiração usa TTL de **120s**, processos e identidades novos.

Comandos do runner, com marcadores não executáveis:

```text
aurora-connect-invite-audit.exe host-external-ipv6-interactive <plano próprio> <SHA256 aprovado>
aurora-connect-invite-audit.exe guest-external-ipv6-interactive <peer público do anfitrião> 127.0.0.1:42010
```

O convidado usa o launcher pinado do kit, com o peer preenchido pelo coordenador:

```text
powershell -NoProfile -ExecutionPolicy Bypass -File .\guest-47b.ps1 <peer público do anfitrião>
```

O script verifica o binário com SHA-256 pela API .NET e o executa diretamente no console atual, sem redirecionamento, mudança de ambiente, janela nova, aprovação ou reinício automáticos. O modo interativo mantém o filtro de IPv6 externo; `host`/`guest` continuam restritos a loopback. Não existe detecção/fallback automático de modo, relay ou endereço.

## Ensaio de entrada, replay e revogação

1. Anfitrião inicia o runner e recebe `audit_host_identity`. O convidado inicia o seu com esse pin e devolve `audit_guest_identity`. Manter o mesmo terminal/processo aberto. Não usar `ready`, comando do fluxo histórico.
2. Depois de receber o peer convidado, anfitrião inicia o echo, fornece somente o peer público ao runner, confere a revisão integral e digita `approve <digest>`. Só então recebe `audit_invitation` e `audit_host_ready`. Aguardar host_ready antes de iniciar o convidado seria circular.
3. Convidado recebe convite e `approve <digest>` em duas linhas, cola no mesmo terminal **enquanto o runner estiver aberto** e aguarda `audit_guest_ready`. O runner valida assinatura, destinatário, pin independente, modo e serviço antes de conectar. O perfil 47b dá 600s por prompt, sem ultrapassar a lease. Se aparecer `audit_failed`, `kit_guest_finished` ou o prompt `PS ...>`, não colar mais dados: devolver o último evento e aguardar tentativa nova coordenada, sem retry automático.
4. No segundo terminal do convidado, executar uma vez `powershell -NoProfile -ExecutionPolicy Bypass -File .\probe-tcp-47.ps1`. O helper envia, recebe e compara 1MiB por direção. Devolver somente `kit_tcp_probe`, esperado `passed=true` e `cleanup=true`. O anfitrião confere um destino TCP e 2097152bytes.
5. Convidado envia `disconnect` e aguarda rebind. Anfitrião registra `denied=0`. Convidado envia `retry-replay`; espera `replayed_request`, mesma identidade e aprovação do ensaio. Anfitrião observa `denied=1` e nenhum novo destino/byte.
6. Anfitrião envia `revoke`, confirma `endpointRetained=true` e pede `retry-revoked`. O convidado deve observar `expired_or_revoked`; anfitrião confirma `denied=2`, sem novo destino/byte. Recusa local isolada não é prova de resposta remota.
7. Convidado envia `stop`, devolve `audit_guest_stopped`/cleanup e confirma retorno ao PowerShell. Coordenador encerra host e echo, confere exit0, recursos próprios liberados e rebind. Não encerra outro processo encontrado por nome.

Para cancelar antes da prontidão, usar Ctrl+C e registrar o evento final. Depois de conectado, preferir `stop` e conferir cleanup. O runner limita entradas a 4096B úteis, uma fila de entrada, 32 comandos e quatro tentativas negativas. Recusa ensaios negativos próximos da expiração para não atribuir TTL a revogação/replay. Os eventos de troca contêm os dados públicos necessários; diagnósticos compartilháveis e Git não devem armazenar peers, IPs, convites ou payload.

## Ensaio separado de expiração

Repetir entrada e probe com plano de 120s e processos novos, somente quando os dois participantes estiverem prontos. Exigir `audit_guest_ready` **antes da expiração original**. A janela interativa de 600s não aumenta a lease de 120s: uma troca humana lenta pode consumi-la antes da admissão. Nesse caso, não houve sessão ativa para testar e a tentativa não aprova expiração.

Depois da prontidão e do probe, manter o convidado conectado, sem `disconnect`, `revoke` ou `stop`. O TTL de 120s começa na emissão, antes do handshake; por isso seu prazo deve ocorrer antes do idle de 120s contado pelo transporte já conectado. Isso precisa ser confirmado pelos eventos correlacionados, não apenas pela configuração. Não usar TTL de 180s/300s com uma sessão ociosa e atribuir todo fechamento à lease. Heartbeat ou renovação não fazem parte deste kit.

Correlacionar `audit_host_stopped` com `reason=lease_expired`, fechamento observado pelo convidado, `deadlineReached=true`, verificação local posterior e cleanup/rebind. O evento `audit_expiration_checked` identifica explicitamente `source=local_verification_after_observed_close`: não é chamado de resposta remota Expired. O convidado tem margem de 3s apenas para observar fechamento. Fechamento antecipado, ausência de prontidão, timeout ou erro não aprovam expiração.

## Validação local do incremento 47b

Os dois arquivos Rust alterados foram `apps/connect-lab/src/bin/aurora-connect-invite-audit.rs` e `apps/connect-lab/tests/invite_audit_cli.rs`. Dois testes unitários novos verificam os perfis fixos e a recusa de entrada enfileirada após deadline; os gates CLI também recusam prazo arbitrário como argumento adicional e convite loopback no alias externo.

A suíte final integrada com os incrementos simultâneos passou **147 testes**, com quatro opt-ins ignorados; esses ignorados não contam como aprovação. Formatação, Clippy com `-D warnings` e build dos dois executáveis passaram. O teste próprio 47b foi executado depois, explicitamente, em IPv6 pertencente ao mesmo PC: os dois processos originais esperaram 61s nos primeiros prompts, depois concluíram TCP de 1MiB por direção, replay/revogação recusados e cleanup/rebind. Resultado: **1 PASS em 62,15s**, duas negações correlacionadas no host. Não é prova de outra internet.

Comando do opt-in, após revalidar e fornecer o endereço próprio somente pela variável de ambiente `AURORA_CONNECT_OWN_EXTERNAL_IPV6_TEST_ADDR`:

```powershell
cargo test --offline --locked --manifest-path apps/connect-lab/Cargo.toml --test invite_audit_cli own_ipv6_interactive_two_original_processes_wait_past_sixty_seconds -- --ignored --exact --nocapture --test-threads=1
```

O kit final passou quatro checks reais do launcher em PowerShell5.1: pin ausente, pin inválido, argumento adicional e execução pinada com uma identidade, perfil600s e EOF antes de qualquer endpoint. A primeira fixture do argumento adicional encontrou erro do binder antes do corpo do script; o novo launcher passou a tratar a entrada no próprio guard, sem prompt obrigatório nem diagnóstico bruto. Repetição após normalizar CRLF passou com a mesma evidência. Probe e echo conservam os bytes e as seis provas correspondentes do kit47, sem serem apresentados como novos testes executados.

Logs privados: `.aurora/connect-validation/connect-fragment-full-final-2.log`, `connect-invite-forty-seven-b-own-ipv6.log`, `connect-invite-kit-47b-check-final.log` e `connect-invite-kit-47b-check-crlf.log`. A tentativa remota anterior ficou em `external-invites47-replay-d39841b05f2c43eca22ae5c0cdf126c3/closed.json`, sob o mesmo diretório ignorado. Esses registros não contêm convite ou identidade real em sua evidência compartilhável.

## Evidência, limites e recuperação

### Recusa de validade e diferença de relógios — 08/10/2026

Uma nova tentativa 47b verificou a cópia canônica, assinatura Ed25519 e digest antes de enviar. O convidado relatou `expired_or_revoked` antes da revisão/prontidão; host permaneceu com zero peers, recusas, destinos e bytes. Host e echo encerraram normalmente com exit 0/rebind, sem tráfego. Registro privado `external-invites47b-replay-a4314bf4ef1b42db8b3c5719d6122d82/closed.json`, **NOT_APPROVED**. Não houve revogação anterior à falha; o código agrupa vencimento e emissão futura, não identifica uma causa temporal sozinho.

Medição posterior, somente leitura, com `w32tm /stripchart /computer:time.windows.com /samples:3 /dataonly`: offsets do anfitrião aproximadamente -75,856s e do convidado -1,395s. A diferença observada é compatível com emissão que ainda parecia futura no convidado; não foi capturado o instante exato da verificação remota. Uma consulta remota posterior dentro da janela não demonstra o estado anterior. Sem diagnóstico de NAT ou relaxamento dos validadores. W32Time no anfitrião foi observado parado/manual, Type NoSync; correção pontual exige plano, snapshot, autorização e recuperação, preparada separadamente antes de nova tentativa.

Registrar separadamente relato remoto, topologia confirmada pelo usuário, observações locais, bins/hashes, prazo, bytes, contadores e cleanup. Se falhar, conservar o código/último evento, encerrar recursos próprios e preparar nova tentativa; não atribuir causa a NAT/firewall sem evidência. Não pedir senhas, tokens, chaves privadas ou gravações.

Após autorização específica e aceite do UAC pelo usuário, a correção pontual foi executada no anfitrião: -75,8641067s, seguida de três offsets 0,0154518/0,0172183/0,0171261s. Helper/filho saíram 0, mutex liberado, configurações preservadas e nenhum rollback necessário. Snapshot, plano e resultado foram gravados em pasta privada nova. Isso corrige o relógio observado; ainda não aprova a nova tentativa de convite nem sincronização permanente.

Este ensaio comprova apenas convites e TCP sintético. UDP mantém suas provas anteriores; Minecraft, interface Tauri, SVC, Plasmo, contas mistas, NAT/CGNAT, relay e estabilidade exigem seus próprios testes. [Contrato](engineering/AURORA_CONNECT_SPEC.md), [estado e comandos executados](task-aur-r4-connect.md), [integração desktop](aurora-connect-desktop.md).

Rollback operacional: `stop` em cada runner próprio e no echo, seguido da conferência de liberação. EOF/Ctrl+C também encerram recursos, mas não substituem a comprovação do teste planejado. Não há configuração pessoal a restaurar; conservar kit/evidências. Rollback de código é revert local revisado somente do incremento, sem reset/clean ou substituição de artefatos históricos.
