# Aurora Connect — entrada no mundo entre duas internets

Estado: **PARCIAL**. Em 08/10/2026, o fluxo manual do incremento 44 teve entrada no mundo, caminhada e terreno normal relatados pelo convidado em outro computador/outra internet. O agente observou separadamente a população do mesmo servidor passar de 0 para 1 e voltar a 0; convidado, host e Java encerraram, com EULA restaurada e portas livres locais. Recursos continuam experimentais e desativados no produto. A versão exata do cliente, loader e Core/Skins não foi observada; não homologa dois jogadores, contas mistas, voz, estabilidade ou a matriz inteira. [Evidências e limites do incremento 44](task-aur-r4-connect.md#quadragésimo-quarto-incremento--entrada-externa-no-mundo-pelo-fluxo-manual).

O fluxo manual abaixo é o procedimento validado nessa amostra. O supervisor 43 passou testes locais, mas teve duas falhas externas antes de `guest_ready`, ambas com `timed_out` e zero STATUS. A segunda durou 13278 ms de sessão, com cleanup relatado. O agente encerrou somente a ponte, manteve o mesmo Java em execução e preparou nova identidade/autorização para o teste manual que passou; não reiniciou o estágio nem renovou o prazo do Java. Duas auditorias não determinaram a causa da diferença. Não atribuir a falha ao wrapper, firewall ou relay por essa comparação; o supervisor continua **PROTÓTIPO**.

Histórico preservado: as duas tentativas GUI 41 não aprovaram entrada no mundo. Na segunda, o usuário apagou a saída do erro; código e causa não foram recuperados. Não solicitar novamente esse erro nem substituí-lo pela falha da primeira tentativa. O supervisor 43 mantém uma identidade e resumo persistido. Passou 139 fixtures e smoke no mesmo PC/IPv6 próprio, com nove STATUS reais, stop e negativos; snapshots 7182/e48e e seus limites estão no [registro 43](task-aur-r4-connect.md#quadragésimo-terceiro-incremento--supervisor-do-convidado-e-resumo-persistido). Esse ensaio local não usou outro computador/internet ou cliente gráfico; naquele ponto, a entrada externa no mundo ainda estava pendente.

Em 08/10, a tentativa externa com o supervisor 43 falhou com `mg_native_failed`/`timed_out` antes de `guest_ready`, zero STATUS. O convidado relatou resumo salvo e cleanup do filho sem forçar; host e Java tiveram encerramento normal, EULA exata e portas livres observados localmente. O ping remoto respondeu 3/3, 145–150 ms, média de 147 ms, mas isso comprova somente ICMP. IPv6 candidato no preflight e permissão UDP correspondente na auditoria somente leitura do firewall Private do host não comprovam entrega UDP ao processo.

No diagnóstico seguinte, os helpers PktMon 1 e 2 recusaram antes do monitoramento, respectivamente por nome de filtro truncado e parsing LF que não aceitava CRLF. O agente comparou exatamente o filtro próprio e recuperou monitor parado/filtros vazios. A versão 3, com nome curto e parsing corrigido, passou três grupos de fixtures PowerShell 5.1 e QA independente. Foi executada somente com contadores de interfaces, IPv6/UDP42000: o usuário enviou quatro datagramas, 64/1200/64/1200 B, total de 2528 B de payload. A janela observou Rx de quatro pacotes/2776 B em dois componentes e Tx zero; são os mesmos quatro, não oito. Sem listener ou resposta esperada, isso comprova chegada UDP à interface, não recebimento pelo processo ou QUIC/TLS/Hello. A parada normal ocorreu em 95103 ms, com monitor parado/filtro próprio removido, sem conflito, ETL, payload armazenado ou mudança de firewall.

Na segunda amostra externa de 08/10, somente do transporte autenticado, o convidado usou `guest-tcp` do kit 38 e o host usou `host-plan` do binário 42, com prazo de 300 s. Apenas `game/tcp` para `127.0.0.1:43001` estava aprovado, sem Java ou listener nesse destino. O usuário retornou `guest_ready` em modo TCP, listener `127.0.0.1:42010`, e depois `guest_stopped`; o agente observou separadamente o host encerrar normalmente com exit 0. O PktMon versão 4 observou Rx/Tx em todos os componentes, com filtro IPv6/UDP42000 e somente contadores. Parou normalmente em 104240 ms, com monitor parado/filtro removido, sem conflito, ETL ou mudança de firewall. Os contadores não comprovam bytes TCP de aplicação ou UDP de aplicação, nem devem ser somados entre componentes como tráfego único. Não houve consulta STATUS ou entrada no mundo nesse ensaio.

A causa do timeout anterior continua indeterminada; a prontidão posterior não demonstra correção de firewall ou relay. Relay permaneceu desligado. Esse diagnóstico foi seguido pelo ensaio manual do incremento 44 descrito no início do guia; seus resultados não alteram os snapshots anteriores. Não repetir identidades ou executar o roteiro GUI com dados de uma sessão encerrada.

## Preparação do convidado

No computador da outra internet, usar o Aurora Launcher para criar uma instância nova, com nome ainda não utilizado, Minecraft **1.21.1** e **Fabric**. O fluxo do Aurora pode instalar Core/Skins automaticamente; registrar depois as versões efetivas sem assumir que o teste os homologou. O loader do cliente também deve ser registrado: o Aurora pode selecionar uma versão estável diferente do Fabric0.16.14 fixado no servidor.

Abrir até o menu principal. Informar somente o nick público exato, respeitando maiúsculas/minúsculas, para a lista de acesso do servidor. Não enviar credenciais, UUIDs, tokens ou logs completos. O agente só alcança o computador anfitrião; a operação da janela do Minecraft e do PowerShell remoto depende do usuário.

Não alterar uma instância existente ou instalar mods de voz para este ensaio. A instalação na instância nova é feita pelo fluxo normal autorizado do Aurora. A aceitação de EULA/MSA para o laboratório já foi fornecida pelo usuário.

## Preparação do anfitrião — responsabilidade do agente

O agente prepara um estágio novo do perfil GUI, revisa plano/hash e whitelist de um ou dois nicks públicos aprovados. Servidor Minecraft1.21.1/Fabric0.16.14/Java21, nove artefatos fixados, sem mods no servidor. Apenas a pasta descartável recebe `online-mode=false`, whitelist ativa, compressão desligada e `enforce-secure-profile=false`. O UUID da whitelist segue o algoritmo offline do próprio Minecraft, com o nick exato; o UUID do perfil do launcher não substitui esse cálculo.

Nenhum JSON externo fornece comandos ou URLs ao executor. Os nomes e os bytes da whitelist fazem parte do plano e são reconstruídos/conferidos antes de execução. Cada Java exige estágio novo, consentimento existente, snapshot/restauração da EULA e bind próprio em `127.0.0.1:43001`. O perfil GUI tem duração fixa de900s; os modos anteriores mantêm seus limites.

O helper privado GUI prepara identidades antes de iniciar o Java. O agente fornece o IPv6 atual e o peer público temporário diretamente no chat. Recebe a linha `guest_identity` do mesmo terminal remoto e a salva na sessão correspondente. Após `server_ready`, publica o registro local de propriedade do runtime e aguarda `host_ready`. Autoriza somente `game/tcp`, destino127.0.0.1:43001, transporte QUIC/UDP42000. Não há serviço UDP de aplicação ou relay neste ensaio.

Não usar identidades, aprovações, estágios ou arquivos de uma sessão anterior. O helper da ponte não inicia nem encerra Java. O executor do servidor mantém essa responsabilidade e seu rollback.

<a id="convidado--fluxo-preferencial-com-um-terminal"></a>

## Convidado — supervisor 43 experimental

Este é o fluxo de um terminal preparado no incremento 43, preservado para investigação. As falhas externas acima continuam abertas; para o procedimento validado no incremento 44, usar o fluxo manual adiante quando o agente preparar uma nova sessão. Não executar os dois fluxos com a mesma identidade ou autorização.

O agente prepara e entrega a cópia revisada de [connect-minecraft-guest-lab.ps1](../scripts/connect-minecraft-guest-lab.ps1), junto do hash correspondente, para a pasta `C:/AuroraConnectLab38`. Não substituir o executável do kit38: o supervisor exige SHA256 `5c1f3b3e61c2a975401d43f5c5d5568d9801a7252b7381451acb8026e6fd2030` antes de cada filho. O debug posterior42 tem outro hash e não é aceito silenciosamente. O script é experimental; execução remota depende de sua validação e entrega pelo agente, não apenas de existir este roteiro.

A cópia recebida em 08/10 foi conferida separadamente: 26384 B, SHA-256 `cbfa1c7652c3f9b003536cd7cfd5038f901433af51d822c9e6d40a56a02e6636`, somente terminadores CRLF no lugar de LF, sem BOM; 139 fixtures PowerShell 5.1 PASS. O script LF original mantém o hash e48e registrado no task. Não desativar a conferência de hash ou aceitar outras diferenças por analogia; o executável do kit não mudou.

Com o arquivo entregue/conferido e o Minecraft no menu, abrir Windows PowerShell64bits com console interativo e executar uma única vez:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:/AuroraConnectLab38/connect-minecraft-guest-lab.ps1
```

Colar o IPv6 atual e o peer público fornecidos diretamente pelo agente no chat. O supervisor inicia um único `guest-tcp`; enviar somente a linha `guest_identity` exibida, mantendo o mesmo terminal aberto. Após a confirmação `host_ready`/autorização pelo agente, digitar `ready` e Enter ali. Não reiniciar o comando: isso criaria outra identidade. O supervisor não autoriza o peer pelo anfitrião nem reutiliza a autorização de outra sessão.

Depois de validar `guest_ready`, o supervisor executa automaticamente STATUS/pong767 pela ponte `127.0.0.1:42010`, inicialmente e depois a cada20s, no máximo45consultas, sem retries ou consultas acumuladas. Não é necessário um segundo PowerShell. Quando aparecer a confirmação da primeira consulta, entrar no Minecraft por **Multijogador → Conexão direta → `127.0.0.1:42010`**. Avisar se o mundo renderizou e se conseguiu caminhar/ver terreno; o agente verifica separadamente a contagem do servidor. STATUS periódico não observa a janela, movimento ou áudio e não homologa uma sessão GUI.

Limites operacionais fixos: preparação600s, sessão900s e total1500s, com cleanup próprio após parada/falha. Recolhimento peloSO pode ultrapassar o prazo operacional. Não estendem a lease do host ou o servidorJava, não mudam a inatividade do transporte e não garantem disponibilidade por900s. Falha de consulta/evento encerra com diagnóstico fixo, sem diagnósticoNAT ou tentativa automática com nova identidade. Não instala dependências, altera firewall/configurações pessoais ou iniciaJava; autoriza apenas `game/tcp`, sem voz/UDP de aplicação/relay.

Para encerrar normalmente, sair do mundo e avisar o agente; depois digitar `stop` e Enter no mesmo terminal. Antes de `ready`, `stop` ou Ctrl+C cancela. O resumo final `minecraft_guest_closed` registra resultado/cleanup, códigos fixos, contagens e a última consulta numérica; fica salvo por criação exclusiva em `attempt-<id>/summary.json` dentro do kit. Ele não inclui IP, peer, nick, credenciais ou saída bruta, e mantém `worldJoined=false`/`guiObserved=false`: a observação humana fica separada. Devolver esse resumo redigido e o relato de entrada/saída; não é preciso recuperar rolagem apagada do console. Parada forçada, falha ou resumo não salvo continuam explícitos, sem transformar cleanup em sucesso do jogo.

<a id="alternativa-manual-histórica--código1-no-powershell"></a>

## Fluxo manual — código 1 no PowerShell

Os códigos 1/2 preservam o procedimento das tentativas 41 e foram usados em uma nova sessão validada no incremento 44. O histórico de falhas permanece registrado. Executar somente após o agente preparar identidade, autorização e servidor atuais; não combinar com o supervisor 43 na mesma tentativa ou abrir outra identidade para substituir uma sessão em andamento.

O kit38 já existente em `C:/AuroraConnectLab38` é suficiente. Executar uma única vez, quando o agente fornecer dados novos:

```powershell
& {
$ErrorActionPreference = 'Stop'
Set-Location C:/AuroraConnectLab38
$lab = (Resolve-Path ./aurora-connect-lab.exe).Path
$expected = '5c1f3b3e61c2a975401d43f5c5d5568d9801a7252b7381451acb8026e6fd2030'
if ((Get-FileHash -LiteralPath $lab -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'LAB_HASH_MISMATCH' }
$hostAddress = Read-Host 'IPv6 do novo teste Minecraft'
$hostPeer = Read-Host 'Peer publico do novo teste Minecraft'
if ($hostPeer -cnotmatch '^[a-f0-9]{64}$') { throw 'LAB_PEER_INVALID' }
$address = $null
if ($hostAddress.Contains('%') -or $hostAddress.Contains('[') -or $hostAddress.Contains(']') -or -not [System.Net.IPAddress]::TryParse($hostAddress, [ref]$address) -or $address.AddressFamily -ne [System.Net.Sockets.AddressFamily]::InterNetworkV6) { throw 'LAB_IPV6_INVALID' }
$bytes = $address.GetAddressBytes()
$documentation = $bytes[0] -eq 0x20 -and $bytes[1] -eq 0x01 -and $bytes[2] -eq 0x0d -and $bytes[3] -eq 0xb8
$documentationNew = $bytes[0] -eq 0x3f -and $bytes[1] -eq 0xff -and ($bytes[2] -band 0xf0) -eq 0
if ($address.ScopeId -ne 0 -or ($bytes[0] -band 0xe0) -ne 0x20 -or $documentation -or $documentationNew) { throw 'LAB_IPV6_NOT_CANDIDATE' }
& $lab guest-tcp $hostPeer ('[' + $address.ToString() + ']:42000') $hostPeer 127.0.0.1:42010 900
if ($LASTEXITCODE -ne 0) { throw 'LAB_GUEST_FAILED' }
}
```

Enviar a linha pública `guest_identity` ao agente e manter o mesmo terminal aberto. Repetir o código gera outra identidade. O agente pode fornecer o mesmo bloco com os dois valores públicos já preenchidos para a sessão atual; nunca copiar valores de tentativas encerradas.

Preparar o segundo PowerShell e o código2 abaixo **antes** de digitar `ready`. Somente após a confirmação de autorização do anfitrião, digitar **`ready`** e Enter no primeiro terminal. Conferir o evento `guest_ready`, com `mode=tcp` e `tcp=127.0.0.1:42010`, e executar a consulta preparada imediatamente. Não é necessário esperar outra mensagem do agente entre esse evento e a consulta. `guest_ready` é a resposta, não o comando a digitar.

Os900s são o prazo máximo da sessão, não uma garantia de prontidão ociosa: o transporte tem limite de inatividade120s. O kit38 pode continuar mostrando uma mensagem antiga após perder a conexão; uma consulta posterior deve verificar disponibilidade novamente. Essa limitação foi encontrada por auditoria de código e não identifica sozinha a causa de uma falha remota. Não mudar timeouts, autorizações ou configuração para contorná-la.

<a id="alternativa-manual-histórica--consulta-e-entrada-no-mundo"></a>

## Fluxo manual — consulta e entrada no mundo

Depois de `guest_ready`, executar o código2 em outro PowerShell:

```powershell
& {
$ErrorActionPreference = 'Stop'
Set-Location C:/AuroraConnectLab38
$lab = (Resolve-Path ./aurora-connect-lab.exe).Path
$expected = '5c1f3b3e61c2a975401d43f5c5d5568d9801a7252b7381451acb8026e6fd2030'
if ((Get-FileHash -LiteralPath $lab -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'LAB_HASH_MISMATCH' }
& $lab minecraft-status 127.0.0.1:42010 localhost 43001 767
if ($LASTEXITCODE -ne 0) { throw 'LAB_MINECRAFT_STATUS_FAILED' }
}
```

Esperado: `minecraft_status_received`, protocolo767 e `protocolMatchesRequested=true`. A consulta deve inicialmente mostrar zero jogadores. Ela confirma a disponibilidade do servidor antes de abrir a conexão do jogo; não comprova entrada no mundo.

No Minecraft já aberto, escolher **Multijogador → Conexão direta**, digitar **`127.0.0.1:42010`** e entrar. Usar conexão direta evita salvar um servidor na lista pessoal. Não digitar o IPv6 público no Minecraft: ele pertence ao transporte Connect.

Ao ver o mundo, avisar o agente e permanecer conectado para que ele confira a contagem local. Caminhar e confirmar que o terreno aparece e os comandos de movimento respondem. O agente registra separadamente o relato de renderização/interação e a observação de contagem do servidor. Uma permanência curta não comprova estabilidade, desempenho ou multiplayer com dois jogadores.

Se houver desconexão, informar só a mensagem curta visível e a fase: antes de `guest_ready`, consulta STATUS ou entrada no mundo. Não desativar whitelist, mudar o nick ou trocar de instância para contornar uma falha sem um novo plano revisado.

## Encerramento e recuperação

**Convidado:** depois da observação, sair do servidor pelo menu do jogo e avisar. O agente confere a contagem voltar a zero. No fluxo manual validado, digitar `stop` no primeiro PowerShell e aguardar `guest_stopped`; se cancelar antes de `ready`, usar Ctrl+C e confirmar o retorno ao prompt. No supervisor 43 experimental, digitar `stop` no único PowerShell e aguardar `minecraft_guest_closed`/resumo salvo com cleanup; cancelamento pré-ready não comprova `guest_stopped`.

**Anfitrião:** o agente solicita stop ao supervisor da ponte, aguarda seus filhos próprios e envia `stop` ao executor Minecraft. Confere `server_stopped`, processo recolhido, EULA restaurada exatamente, portas43001TCP/42000UDP livres e ausência dos próprios agentes. Nunca encerra um processo alheio para liberar porta. Conflito de restauração preserva snapshot/evidências para revisão.

O mundo e os arquivos gerados ficam na pasta descartável preservada, sem reuso. O cliente pode ficar no menu principal ou ser fechado pelo usuário. Nenhuma configuração de firewall, roteador, conta ou instância pessoal é alterada pelo agente. Prazo, falha ou parada forçada continuam sendo falhas mesmo com limpeza posterior.

Devolver somente: resultado STATUS/eventos disponíveis, se entrou e conseguiu caminhar/ver o terreno, versão do cliente/loader/Core/Skins se disponível, confirmação de saída/encerramento e de que usou outro computador/outra internet. Se estiver usando o supervisor 43 em ensaio autorizado, devolver seu resumo redigido. Não enviar IPs/peers novamente depois da troca inicial, credenciais, mundo ou logs brutos. A causa da segunda tentativa 41 continua não confirmada; não é necessário responder novamente sobre a saída já apagada.
