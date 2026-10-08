# Convites assinados entre duas redes — ensaio47

Estado: **PROTÓTIPO**. O runner `aurora-connect-invite-audit` usa os convites e transporte reais, com a mesma identidade do convidado mantida em RAM durante desconexão e tentativas negativas. Os ensaios automatizados de loopback e IPv6 próprio passaram; a execução em dois computadores/outra internet ainda é **PRECISA DE TESTE**. A prova gráfica44 usou o fluxo manual anterior e não aprova este convite.

## Preparação

O coordenador prepara e testa um kit privado novo, com executáveis fixados por SHA-256, procedimento e helpers TCP. O manifesto registra seu estado de criação; não será reescrito como prova remota. Não substituir kit38 ou supervisor43. Não há instalador, Java, mod, mudança de firewall/roteador/configuração pessoal, relay ou custo. O convidado copia o kit para uma pasta local regular e confere o hash fornecido antes de executar. Ferramentas do coordenador alcançam apenas o anfitrião.

O anfitrião revalida seu endereço IPv6, cria plano/hash novo autorizando somente `game/tcp` para o echo próprio em `127.0.0.1:43001` e prazo300s. O echo permite uma conexão, 1MiB por direção e encerramento explícito. A segunda tentativa, de expiração ativa, usa plano120s e processos/identidades novos. O contrato geral continua limitado a600s, sem renovação.

Comandos do runner, com marcadores não executáveis:

```text
aurora-connect-invite-audit.exe host-external-ipv6 <plano próprio> <SHA256 aprovado>
aurora-connect-invite-audit.exe guest-external-ipv6 <peer público do anfitrião> 127.0.0.1:42010
```

Os modos `host`/`guest` conservam restrição a loopback. Não existe detecção/fallback automático de modo, relay ou endereço.

## Ensaio de entrada, replay e revogação

1. Anfitrião inicia o runner e recebe `audit_host_identity`. O convidado inicia o seu com esse pin e devolve `audit_guest_identity`. Manter o mesmo terminal/processo aberto. Não usar `ready`, comando do fluxo histórico.
2. Anfitrião fornece somente o peer público ao seu runner, confere a revisão integral e digita `approve <digest>`. Só então recebe `audit_invitation` e `audit_host_ready`. Aguardar host_ready antes de iniciar o convidado seria circular.
3. Convidado recebe convite e `approve <digest>` em duas linhas, cola no mesmo terminal e aguarda `audit_guest_ready`. O runner valida assinatura, destinatário, pin independente, modo e serviço antes de conectar. Cada prompt de preparação tem60s; expiração requer tentativa nova coordenada, sem retry automático.
4. No segundo terminal do convidado, o helper TCP envia/recebe/compara1MiB. Devolver somente o resumo `kit_tcp_probe`. O anfitrião confere um destinoTCP e2097152bytes.
5. Convidado envia `disconnect` e aguarda rebind. Anfitrião registra `denied=0`. Convidado envia `retry-replay`; espera `replayed_request`, mesma identidade e aprovação do ensaio. Anfitrião observa `denied=1` e nenhum novo destino/byte.
6. Anfitrião envia `revoke`, confirma `endpointRetained=true` e pede `retry-revoked`. O convidado deve observar `expired_or_revoked`; anfitrião confirma `denied=2`, sem novo destino/byte. Recusa local isolada não é prova de resposta remota.
7. Convidado envia `stop`, devolve `audit_guest_stopped`/cleanup e confirma retorno ao PowerShell. Coordenador encerra host e echo, confere exit0, recursos próprios liberados e rebind. Não encerra outro processo encontrado por nome.

O runner limita entradas a4096B úteis, uma fila de entrada,32comandos e quatro tentativas negativas. Recusa ensaios negativos próximos da expiração para não atribuir TTL a revogação/replay. Os eventos de troca contêm os dados públicos necessários; diagnósticos compartilháveis e Git não devem armazenar peers, IPs, convites ou payload.

## Ensaio separado de expiração

Repetir entrada e probe com plano120s e processos novos. Manter o convidado conectado, sem disconnect/revoke/stop. O prazo começa na emissão antes da conexão, evitando um ensaio180s ocioso que poderia fechar pelo idle120s antes da lease.

Correlacionar `audit_host_stopped` com `reason=lease_expired`, fechamento observado pelo convidado, `deadlineReached=true`, verificação local posterior e cleanup/rebind. O evento `audit_expiration_checked` identifica explicitamente `source=local_verification_after_observed_close`: não é chamado de resposta remota Expired. O convidado tem margem3s apenas para observar fechamento. Fechamento antecipado, timeout ou erro não aprovam expiração.

## Evidência, limites e recuperação

Registrar separadamente relato remoto, topologia confirmada pelo usuário, observações locais, bins/hashes, prazo, bytes, contadores e cleanup. Se falhar, conservar o código/último evento, encerrar recursos próprios e preparar nova tentativa; não atribuir causa a NAT/firewall sem evidência. Não pedir senhas, tokens, chaves privadas ou gravações.

Este ensaio comprova apenas convites e TCP sintético. UDP mantém suas provas anteriores; Minecraft, interface Tauri, SVC, Plasmo, contas mistas, NAT/CGNAT, relay e estabilidade exigem seus próprios testes. [Contrato](engineering/AURORA_CONNECT_SPEC.md), [estado e comandos executados](task-aur-r4-connect.md), [integração desktop](aurora-connect-desktop.md).

Rollback operacional: `stop` em cada runner próprio e no echo, seguido da conferência de liberação. EOF/Ctrl+C também encerram recursos, mas não substituem a comprovação do teste planejado. Não há configuração pessoal a restaurar; conservar kit/evidências. Rollback de código é revert local revisado somente do incremento, sem reset/clean ou substituição de artefatos históricos.
