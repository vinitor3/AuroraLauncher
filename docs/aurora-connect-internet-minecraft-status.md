# Aurora Connect — STATUS Minecraft entre duas internets

Estado: **PRECISA DE TESTE** externo. Complementa o ensaio39 de TCP/UDP sintéticos. Consulta o servidor Minecraft real pelo túnel autenticado, com STATUS e pong. Login, entrada no mundo, voz, clientes gráficos/mistos, Core/Skins e interface do launcher ficam fora deste ensaio.

Usa o mesmo kit38 já copiado para `C:/AuroraConnectLab38`, executável Windows x64 SHA256 `5c1f3b3e61c2a975401d43f5c5d5568d9801a7252b7381451acb8026e6fd2030`. Não instalar nada no convidado. Sessões e peers anteriores estão encerrados; o agente fornece dados novos. Relay continua desligado.

## Anfitrião — o agente executa neste PC

O agente prepara uma pasta descartável nova com os nove artefatos fixados de Minecraft1.21.1/Fabric0.16.14/Java21, sem mods, total55.582.631B. Revisa plano/hash, argumentos, ambiente mínimo e diffEULA; a aceitação dos termos já foi dada pelo usuário. O executor existente exige hashes atuais, snapshot da EULA, marcador sem reuso, bindTCP127.0.0.1:43001 pertencente ao próprio Java e STATUS/pong767 antes de readiness. `online-mode=false` é a preferência autorizada para este servidor próprio; whitelist permanece ativa e nenhuma conta é usada nessa consulta.

Depois de `server_ready`, inicia o helper privado `external-minecraft-host-session.ps1` com o PID do Java próprio observado localmente. O helper confere novamente a propriedade do bind e aprova somente `game/tcp`, revisão1/capability reliableStream, destino127.0.0.1:43001. Transporte em IPv6 candidato atual/UDP42000, sem serviço UDP de aplicação. Plano/hash/serviços são conferidos antes de autorizar o peer. O helper nunca inicia nem encerra Java; o executor do servidor conserva essa responsabilidade.

O prazo operacional do Java é300s, incluindo boot; o encerramento e a liberação pelo sistema operacional são aguardados e podem ultrapassar esse prazo. Por isso os dois participantes devem estar presentes antes do início. O agente aguarda a identidade pública do convidado pelo arquivo privado `guest-peer.txt`, confirma `host_ready` e só então manda digitar `ready`. Se o prazo acabar, encerrar e preparar outra pasta/sessão; não reutilizar o estágio executado ou os peers.

## Convidado — primeiro terminal

Com os dados novos do arquivo ANFITRIAO.txt fornecido pelo agente, executar o bloco completo:

```powershell
& {
$ErrorActionPreference = 'Stop'
Set-Location C:/AuroraConnectLab38
$lab = (Resolve-Path ./aurora-connect-lab.exe).Path
$expected = '5c1f3b3e61c2a975401d43f5c5d5568d9801a7252b7381451acb8026e6fd2030'
if ((Get-FileHash -LiteralPath $lab -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'LAB_HASH_MISMATCH' }
$hostAddress = Read-Host 'IPv6 novo do anfitriao, sem colchetes e sem porta'
$hostPeer = Read-Host 'Peer publico novo do anfitriao, 64 hex'
if ($hostPeer -cnotmatch '^[a-f0-9]{64}$') { throw 'LAB_PEER_INVALID' }
$address = $null
if ($hostAddress.Contains('%') -or $hostAddress.Contains('[') -or $hostAddress.Contains(']') -or -not [System.Net.IPAddress]::TryParse($hostAddress, [ref]$address) -or $address.AddressFamily -ne [System.Net.Sockets.AddressFamily]::InterNetworkV6) { throw 'LAB_IPV6_INVALID' }
$bytes = $address.GetAddressBytes()
$documentation = $bytes[0] -eq 0x20 -and $bytes[1] -eq 0x01 -and $bytes[2] -eq 0x0d -and $bytes[3] -eq 0xb8
$documentationNew = $bytes[0] -eq 0x3f -and $bytes[1] -eq 0xff -and ($bytes[2] -band 0xf0) -eq 0
if ($address.ScopeId -ne 0 -or ($bytes[0] -band 0xe0) -ne 0x20 -or $documentation -or $documentationNew) { throw 'LAB_IPV6_NOT_CANDIDATE' }
& $lab guest-tcp $hostPeer ('[' + $address.ToString() + ']:42000') $hostPeer 127.0.0.1:42010 300
if ($LASTEXITCODE -ne 0) { throw 'LAB_GUEST_FAILED' }
}
```

Salvar a linha `guest_identity` no arquivo privado indicado. Aguardar confirmação do agente e então digitar `ready`. Esperar `guest_ready` com `mode=tcp` e `tcp=127.0.0.1:42010`. Falha interrompe o ensaio; não executar a consulta sem esse evento.

## Convidado — segundo terminal

Depois da prontidão, executar:

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

Esperado: `minecraft_status_received`, `summary.reportedProtocol=767`, `summary.protocolMatchesRequested=true` e pong válido. Exit0 sozinho não basta: o protocolo reportado precisa corresponder ao solicitado. `localhost`/43001 são campos do handshake Minecraft; a conexão TCP do cliente é feita exclusivamente ao listener local42010 da ponte. O transporte do túnel ainda utiliza QUIC/UDP, embora autorize somente TCP de aplicação. A resposta continua com `runtimeHomologated=false`, `worldJoined=false` e `voice=not_tested`.

## Encerramento e dados de retorno

Convidado: depois de `guest_ready`, digitar `stop` no primeiro terminal e aguardar `guest_stopped`. Se o ensaio for cancelado ainda em `guest_identity`, antes de digitar `ready`, usar `Ctrl+C` e confirmar o retorno ao PowerShell; nessa fase não existe o evento `guest_stopped`. Devolver somente o JSON da consulta ou código de falha, a confirmação do encerramento e de que usou outro computador/outra internet. Não enviar IPs, peers, chaves, credenciais ou logs completos.

Anfitrião: agente para o host Connect pelo supervisor próprio; depois envia `stop` ao executor Node do Minecraft. Aguarda `server_stopped`/exit0, `eulaRestored=true`, liberaçãoTCP43001/UDP42000 e ausência dos processos que criou. Se houver conflito na EULA, preserva snapshot/recuperação e não sobrescreve. O helper da ponte nunca mata o dono da porta43001. Mundo/arquivos da pasta descartável ficam preservados; nenhuma instância pessoal é alterada. Falhas e prazos continuam falhas mesmo que o cleanup funcione.
