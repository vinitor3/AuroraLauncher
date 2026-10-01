# Aurora Connect — entrada no mundo entre duas internets

Estado: **PRECISA DE TESTE**. O incremento40 comprovou STATUS e pong externos; este procedimento exige um cliente gráfico entrar no mundo. Recursos continuam experimentais e desativados no produto. Não homologa dois jogadores, contas mistas, Core/Skins, voz, estabilidade ou a matriz inteira.

## Preparação do convidado

No computador da outra internet, usar o Aurora Launcher para criar uma instância nova, com nome ainda não utilizado, Minecraft **1.21.1** e **Fabric**. O fluxo do Aurora pode instalar Core/Skins automaticamente; registrar depois as versões efetivas sem assumir que o teste os homologou. O loader do cliente também deve ser registrado: o Aurora pode selecionar uma versão estável diferente do Fabric0.16.14 fixado no servidor.

Abrir até o menu principal. Informar somente o nick público exato, respeitando maiúsculas/minúsculas, para a lista de acesso do servidor. Não enviar credenciais, UUIDs, tokens ou logs completos. O agente só alcança o computador anfitrião; a operação da janela do Minecraft e do PowerShell remoto depende do usuário.

Não alterar uma instância existente ou instalar mods de voz para este ensaio. A instalação na instância nova é feita pelo fluxo normal autorizado do Aurora. A aceitação de EULA/MSA para o laboratório já foi fornecida pelo usuário.

## Preparação do anfitrião — responsabilidade do agente

O agente prepara um estágio novo do perfil GUI, revisa plano/hash e whitelist de um ou dois nicks públicos aprovados. Servidor Minecraft1.21.1/Fabric0.16.14/Java21, nove artefatos fixados, sem mods no servidor. Apenas a pasta descartável recebe `online-mode=false`, whitelist ativa, compressão desligada e `enforce-secure-profile=false`. O UUID da whitelist segue o algoritmo offline do próprio Minecraft, com o nick exato; o UUID do perfil do launcher não substitui esse cálculo.

Nenhum JSON externo fornece comandos ou URLs ao executor. Os nomes e os bytes da whitelist fazem parte do plano e são reconstruídos/conferidos antes de execução. Cada Java exige estágio novo, consentimento existente, snapshot/restauração da EULA e bind próprio em `127.0.0.1:43001`. O perfil GUI tem duração fixa de900s; os modos anteriores mantêm seus limites.

O helper privado GUI prepara identidades antes de iniciar o Java. O agente fornece o IPv6 atual e o peer público temporário diretamente no chat. Recebe a linha `guest_identity` do mesmo terminal remoto e a salva na sessão correspondente. Após `server_ready`, publica o registro local de propriedade do runtime e aguarda `host_ready`. Autoriza somente `game/tcp`, destino127.0.0.1:43001, transporte QUIC/UDP42000. Não há serviço UDP de aplicação ou relay neste ensaio.

Não usar identidades, aprovações, estágios ou arquivos de uma sessão anterior. O helper da ponte não inicia nem encerra Java. O executor do servidor mantém essa responsabilidade e seu rollback.

## Convidado — código1 no PowerShell

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

Enviar a linha pública `guest_identity` ao agente e manter o mesmo terminal aberto. Repetir o código gera outra identidade. Somente após a confirmação de autorização do anfitrião, digitar **`ready`** e Enter. Confirmar o evento `guest_ready`, com `mode=tcp` e `tcp=127.0.0.1:42010`. `guest_ready` é a resposta, não o comando a digitar.

## Convidado — consulta e entrada no mundo

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

**Convidado:** depois da observação, sair do servidor pelo menu do jogo e avisar. O agente confere a contagem voltar a zero. Então digitar `stop` no primeiro PowerShell e aguardar `guest_stopped`. Se cancelar ainda em `guest_identity`, antes de `ready`, usar Ctrl+C e confirmar o retorno ao prompt; nessa fase não existe `guest_stopped`.

**Anfitrião:** o agente solicita stop ao supervisor da ponte, aguarda seus filhos próprios e envia `stop` ao executor Minecraft. Confere `server_stopped`, processo recolhido, EULA restaurada exatamente, portas43001TCP/42000UDP livres e ausência dos próprios agentes. Nunca encerra um processo alheio para liberar porta. Conflito de restauração preserva snapshot/evidências para revisão.

O mundo e os arquivos gerados ficam na pasta descartável preservada, sem reuso. O cliente pode ficar no menu principal ou ser fechado pelo usuário. Nenhuma configuração de firewall, roteador, conta ou instância pessoal é alterada pelo agente. Prazo, falha ou parada forçada continuam sendo falhas mesmo com limpeza posterior.

Devolver somente: resultado STATUS, se entrou e conseguiu caminhar/ver o terreno, mensagem curta de falha se houver, versão do cliente/loader/Core/Skins se disponível, confirmação de saída/`guest_stopped` e de que usou outro computador/outra internet. Não enviar IPs/peers novamente depois da troca inicial, credenciais, mundo ou logs brutos.
