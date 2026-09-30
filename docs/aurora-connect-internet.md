# Aurora Connect — ensaio privado entre redes diferentes

Estado: **PRECISA DE TESTE**. Este roteiro testa bytes TCP/UDP entre dois Windows x64 autorizados. Não instala launcher, Minecraft ou mods; não testa contas ou áudio. O laboratório é experimental, sem relay/fallback e sem mudança automática de firewall/roteador. Para computadores na mesma rede, usar [o roteiro LAN](aurora-connect-lan.md).

O agente controla somente o PC desta conversa, que será o **anfitrião**. O **convidado** fica na outra internet. Uma pessoa com autorização nesse computador precisa copiar a pasta de teste e abrir os terminais; não precisa programar, pesquisar ou fornecer acesso remoto/credenciais. Antes de iniciar uma sessão, combinar um intervalo de dez minutos com ambos presentes.

## 1. Preparação do convidado — agora, sem abrir conexão

Copiar a pasta do kit privado indicado no relatório para uma pasta nova `C:/AuroraConnectLab`. Não sobrescrever pasta existente: escolher outro nome e ajustar `Set-Location` nos três blocos abaixo. O kit contém executável debug, preflight, os dois roteiros, licença, avisos e manifesto; não contém Java, servidor, contas ou chaves privadas. Não usar o kit antigo `lan-kit-e6e40dcc`, que corresponde a um executável anterior.

Binário desta revisão:23.641.088bytes, SHA256 `7f8c7ed40e00826f58b5f482f80d1660941d5568d36a2983588488c84d0b9a40`. Preflight SHA256 `eb28b3ae3db5b516a526afa13fd7ad44451b79a44ea652230d0c3c9632cff4ed`. Hash identifica os bytes revisados; não é assinatura de distribuição. Divergência interrompe o procedimento.

Abrir PowerShell comum, sem administrador, e executar o bloco inteiro incluindo `& {` e `}`:

```powershell
& {
$ErrorActionPreference = 'Stop'
Set-Location C:/AuroraConnectLab
$preflight = (Resolve-Path ./connect-network-preflight.ps1).Path
$expected = 'eb28b3ae3db5b516a526afa13fd7ad44451b79a44ea652230d0c3c9632cff4ed'
if ((Get-FileHash -LiteralPath $preflight -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'PREFLIGHT_HASH_MISMATCH' }
$windows = [Environment]::GetFolderPath([Environment+SpecialFolder]::Windows)
$shell = Join-Path $windows 'System32/WindowsPowerShell/v1.0/powershell.exe'
& $shell -NoProfile -ExecutionPolicy Bypass -File $preflight
if ($LASTEXITCODE -ne 0) { throw 'PREFLIGHT_NOT_OBSERVED' }
}
```

O bypass vale apenas para esse processo; não muda política permanente. O script só lê inventário local, emite um JSON e termina. Não abre rede, listeners, firewall ou mapeamentos, não carrega DLL e não grava arquivos. Pode devolver o JSON inteiro: ele contém somente códigos fixos, booleans e contagens, sem IP/gateway/SSID/nome de computador. Se a política da máquina impedir a execução, devolver somente o código; não relaxar política administrada.

Esperado: `status=OBSERVADO`, `windowsX64=true`, `powershellSupported=true` e `vcRuntime140FilePresent=true`. Ausência do runtime significa parar; o agente prepara a opção oficial após observar o impedimento. A presença de `VCRUNTIME140.dll` não prova todas as dependências transitivas ou a execução nesse PC. Não copiar DLL avulsa.

Para o caminho IPv6 preparado abaixo, ambos precisam de `ipv6SameInterfaceCandidate=true`. Isso observa endereço Preferred em2000::/3, excluindo2001:db8 e3fff::/20, e rota padrão Alive na mesma interface. Não garante alcance, firewall ou UDP; `networkReachabilityVerified` e `udpReachabilityVerified` continuam false até um teste real. `ipv4OtherCount` não significa IP público e timeout não identifica NAT/CGNAT.

Se faltar candidato IPv6, devolver o relatório. IPv4 pode ser ensaiado quando houver endereço público diretamente atribuído ao host ou encaminhamento já existente e especificamente autorizado; esse caso precisa de endereços/bind e roteiro próprios. Não mapear portas por tentativa. O modo atual não fornece bootstrap de hole punching entre dois NATs sem caminho inicial; relay permanece desligado e sua elegibilidade tem gate separado.

## 2. Anfitrião — o agente executa neste PC

Após receber o preflight do convidado e combinar o momento, o agente reconfere hash, ausência de conflito nas portas e seleciona um IPv6 Preferred candidato que pertence realmente à interface com rota padrão. O endereço público do roteador nunca é usado como bind local. O anfitrião usa esse bind específico; o convidado atual usa bind wildcard, podendo oferecer candidatas das suas interfaces ao peer TLS. Iroh mantém QNT/multipath32candidatos/8caminhos, mesmo com relay/lookup/observed reports desligados. O controle de caminhos é distinto da autorização dos serviços, que continua exigida antes de abrir destinos TCP/UDP.

O agente inicia um `echo` sintético em TCP127.0.0.1:42001/UDP127.0.0.1:42002, duração600s, e depois `host` com o IPv6 local selecionado entre colchetes e porta42000, os mesmos destinos e duração600s. Subcomandos reais: `echo <tcp> <udp> 600` e `host <bind-IPv6>:42000 <tcp> <udp> 600`; os valores são preenchidos e revisados localmente pelo agente antes da execução. Serviços autorizados são apenas `game` TCP e `aux` UDP sintéticos; não abrem acesso geral à LAN.

Anfitrião entrega privadamente seu IPv6 sem porta e o `peer` público de `host_identity`. Nesse modo manual `session=peer`; o agente confere a igualdade. Recebe o peer público do convidado, confere64hex, digita no stdin do host e aguarda `host_ready`. Só então informa que o convidado pode enviar `ready`. Chaves privadas permanecem nos processos. Pins e IPs não devem entrar nos resultados compartilhados no chat.

## 3. Convidado — primeiro PowerShell, somente quando combinado

Receber IPv6/pin do anfitrião pelo canal privado já disponível. Este bloco confere o executável e recusa hostname, IPv4, link-local, ULA, multicast, IPv6 com scope e faixa de documentação:

```powershell
& {
$ErrorActionPreference = 'Stop'
Set-Location C:/AuroraConnectLab
$lab = (Resolve-Path ./aurora-connect-lab.exe).Path
$expected = '7f8c7ed40e00826f58b5f482f80d1660941d5568d36a2983588488c84d0b9a40'
if ((Get-FileHash -LiteralPath $lab -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'LAB_HASH_MISMATCH' }
$hostAddress = Read-Host 'IPv6 do anfitriao, sem colchetes e sem porta'
$hostPeer = Read-Host 'Peer publico do anfitriao, 64 caracteres hexadecimais'
if ($hostPeer -cnotmatch '^[a-f0-9]{64}$') { throw 'LAB_PEER_INVALID' }
$address = $null
if ($hostAddress.Contains('%') -or $hostAddress.Contains('[') -or $hostAddress.Contains(']') -or -not [System.Net.IPAddress]::TryParse($hostAddress, [ref]$address) -or $address.AddressFamily -ne [System.Net.Sockets.AddressFamily]::InterNetworkV6) { throw 'LAB_IPV6_INVALID' }
$bytes = $address.GetAddressBytes()
$documentation = $bytes[0] -eq 0x20 -and $bytes[1] -eq 0x01 -and $bytes[2] -eq 0x0d -and $bytes[3] -eq 0xb8
$documentationNew = $bytes[0] -eq 0x3f -and $bytes[1] -eq 0xff -and ($bytes[2] -band 0xf0) -eq 0
if ($address.ScopeId -ne 0 -or ($bytes[0] -band 0xe0) -ne 0x20 -or $documentation -or $documentationNew) { throw 'LAB_IPV6_NOT_CANDIDATE' }
& $lab guest $hostPeer ('[' + $address.ToString() + ']:42000') $hostPeer 127.0.0.1:42010 127.0.0.1:42011 600
if ($LASTEXITCODE -ne 0) { throw 'LAB_GUEST_FAILED' }
}
```

Enviar privadamente o peer público de `guest_identity` ao anfitrião. Esperar confirmação de `host_ready`; digitar **`ready`** e Enter, aguardar `guest_ready`. Não enviar ready antes da autorização. Se aparecer pedido de firewall, cancelar e informar; regra temporária só entra depois de plano/diff/escopo/snapshot/autorização/recuperação preparados para o impedimento observado. Não desativar firewall ou aceitar uma permissão ampla improvisada.

## 4. Convidado — segundo PowerShell

Após `guest_ready`, executar:

```powershell
& {
$ErrorActionPreference = 'Stop'
Set-Location C:/AuroraConnectLab
$lab = (Resolve-Path ./aurora-connect-lab.exe).Path
$expected = '7f8c7ed40e00826f58b5f482f80d1660941d5568d36a2983588488c84d0b9a40'
if ((Get-FileHash -LiteralPath $lab -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'LAB_HASH_MISMATCH' }
& $lab probe 127.0.0.1:42010 127.0.0.1:42011
if ($LASTEXITCODE -ne 0) { throw 'LAB_PROBE_FAILED' }
}
```

Esperado: `probe_passed`,1.048.576B TCP por direção,10datagramas íntegros em2origens locais. `guest_ready` sozinho não demonstra encaminhamento. UDP externo bloqueado impede o QUIC mesmo para o serviço TCP. Falha deve ser registrada pelo código e encerrada, sem fallback pago/relay ou diagnóstico de NAT por timeout. Sucesso prova somente esse par de máquinas/rede/momento e esses bytes; não homologa Minecraft, voz ou estabilidade.

## 5. Encerramento e recuperação — ambos

Convidado: no primeiro terminal, digitar **`stop`**, aguardar `guest_stopped` e retorno ao PowerShell. Anfitrião: agente envia `stop` ao host, aguarda `host_stopped`, depois `stop` ao echo e aguarda seu encerramento. Em espera de peer/ready, usar Ctrl+C; stopped só existe após iniciar sessão. O prazo limita sessão iniciada a600s; não deixar processos aguardando pin indefinidamente. Nenhum processo alheio é encerrado.

Não há configuração pessoal/mod, firewall ou roteador alterados por esse roteiro. Chaves efêmeras morrem com os processos; a pasta fica preservada. Uma falha exige novo par de peers/consentimento ao reiniciar; não reaproveitar pins de sessão encerrada. Se futuramente houver regra temporária aprovada, usar o rollback exato daquele plano separado.

Devolver: JSON do preflight, resultado PASS ou código fixo da falha, confirmação de redes diferentes, contagens/durações do probe e confirmação de encerramento de ambos. Informar eventual pedido de firewall/DLL ausente sem logs completos. Não enviar IPs, identidades, senhas, tokens, chaves privadas ou áudio. A prova desbloqueia avaliação de conexão direta entre redes; NAT/CGNAT, relay, jogo, cada mod e UI continuam com gates próprios.
