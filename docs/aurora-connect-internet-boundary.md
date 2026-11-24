# Aurora Connect — fronteira UDP e TCP entre duas internets

Estado: **PARCIAL** — um caso externo passou em01/10/2026, com confirmação do usuário de dois computadores em internets diferentes e encerramento do convidado. O probe nativo passou cap1264/1265 nos dois sentidos, duas origens e TCP1MiB/direção concorrente com cadência200ms; anfitrião/echo encerraram normalmente e portas locais ficaram livres. [Evidência39 e limites](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/task-aur-r4-connect.md#trigésimo-nono-incremento--fronteira-udp-externa-com-tcp-cadenciado): resultado remoto informado pelo usuário, cleanup local observado pelo agente; não mede saturação, MTU física, caminhos/migração, NAT/CGNAT, jogo ou voz. Complementa33 sem promover R4; transporte experimental, autenticação obrigatória e relay desligado. Kit38 original e seu manifesto de criação permanecem imutáveis.

## Preparação — responsabilidade do agente

O agente prepara kit novo separado dos kits anteriores, com executável Windows x64, fonte correspondente, licença/avisos, preflight e este roteiro. Hash do executável: `5c1f3b3e61c2a975401d43f5c5d5568d9801a7252b7381451acb8026e6fd2030`, tamanho 23847936 bytes. Conferir esse hash em ambos os PCs. Não substituir o kit32 nem sua pasta; ele conserva a evidência anterior.

O shell desta conversa só alcança o PC anfitrião. No convidado, uma pessoa autorizada precisa copiar o kit para uma pasta nova `C:/AuroraConnectLab38` e operar dois terminais PowerShell comuns. Não precisa instalar Node/Rust, programar ou pesquisar. O runtime VC140 foi observado no remoto do ensaio33; um PC diferente exige o preflight somenteleitura incluído antes de executar. Se houver DLL ausente ou pedido de firewall, parar e devolver somente o código/nome da DLL; o agente prepara o procedimento específico, sem desativar proteções.

Se trocar de computador, em `C:/AuroraConnectLab38` conferir `Get-FileHash ./connect-network-preflight.ps1 -Algorithm SHA256`: deve ser `eb28b3ae3db5b516a526afa13fd7ad44451b79a44ea652230d0c3c9632cff4ed`. Só com esse hash, executar `powershell.exe -NoProfile -ExecutionPolicy Bypass -File ./connect-network-preflight.ps1`. Esperado: `OBSERVADO`, Windows/Powershell64Bit e VC140 presentes, candidato IPv6 na interface com rota. O bypass vale apenas para o processo; o preflight não comprova alcance TCP/UDP. No mesmo PC do ensaio33, a execução anterior já forneceu esse inventário.

## Anfitrião — o agente executa aqui

1. Reconfere binário/hash, portas TCP42001/UDP42002/UDPtransporte42000 livres e IPv6 Preferred candidato realmente atribuído à interface com rota padrão. Nenhum processo alheio é encerrado.
2. Prepara plano JSON novo, confinado à pasta ignorada: transporteIPv6:42000, prazo600s; gameTCP127.0.0.1:42001; auxUDP127.0.0.1:42002, teto1264 e `authorizedDownstream`100000B/s/100datagramas/s. Revisão1 e capabilities reliableStream/datagram. Confere `plan`, SHA256 e serviços exatos; o convidado não escolhe destinos/política.
3. Inicia `echo-boundary 127.0.0.1:42001 127.0.0.1:42002 600` e `host-plan <plano-novo> <hash-revisado>`, com supervisor próprio. Endereço/pin públicos ficam no arquivo de transferência privado; chaves privadas apenas emRAM.
4. Recebe o peer público do convidado por arquivo/canal privado autorizado. Aceita peer64hex ou linha JSON completa `guest_identity` validada; nunca pede chave privada. Autoriza no stdin do host, aguarda `host_ready`, só então orienta `ready` no convidado. Sessão/pins novos por execução.

O supervisor tem limite600s de preparação e900s totais, arquivo `stop.txt` para encerramento e saída redigida. O agente opera esse lado; você não precisa executar comandos no anfitrião.

## Convidado — primeiro terminal

Com o kit novo copiado e o anfitrião preparado, executar o bloco inteiro:

```powershell
& {
$ErrorActionPreference = 'Stop'
Set-Location C:/AuroraConnectLab38
$lab = (Resolve-Path ./aurora-connect-lab.exe).Path
$expected = '5c1f3b3e61c2a975401d43f5c5d5568d9801a7252b7381451acb8026e6fd2030'
if ((Get-FileHash -LiteralPath $lab -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'LAB_HASH_MISMATCH' }
$hostAddress = Read-Host 'IPv6 do anfitriao, sem colchetes e sem porta'
$hostPeer = Read-Host 'Peer publico do anfitriao, 64 hex'
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

Transferir privadamente a linha `guest_identity` ou somente seu peer público ao arquivo indicado pelo agente. Esperar confirmação do anfitrião; digitar `ready` e Enter. Aguardar `guest_ready` com TCP42010, UDP42011 e `maxDatagramPayload=1264`. Se o teto for menor ou houver falha, não executar o probe: devolver somente o evento redigido/código e encerrar. Esse teto foi aprovado no serviço; não é medição da MTU física.

## Convidado — segundo terminal

Depois de `guest_ready`, executar:

```powershell
& {
$ErrorActionPreference = 'Stop'
Set-Location C:/AuroraConnectLab38
$lab = (Resolve-Path ./aurora-connect-lab.exe).Path
$expected = '5c1f3b3e61c2a975401d43f5c5d5568d9801a7252b7381451acb8026e6fd2030'
if ((Get-FileHash -LiteralPath $lab -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'LAB_HASH_MISMATCH' }
& $lab probe-boundary 127.0.0.1:42010 127.0.0.1:42011 1264
if ($LASTEXITCODE -ne 0) { throw 'LAB_BOUNDARY_PROBE_FAILED' }
}
```

Esperado: `boundary_probe_passed`, quatro ecos1264B íntegros, duas tentativas1265B sem chegar ao endpoint, quatro retornos1265B emitidos e nenhum recebido no convidado, duasorigens e válido após recusa em ambas. TCP1MiB/direção progride duranteUDP. ControleTCP separado consulta contadores do endpoint próprio; não conclui recusa somente por timeout. Prazo60s; TCP é ritmado, não benchmark ou saturação. Emissão significa callback doSO; a observação500ms é finita e não identifica isoladamente o ponto de descarte. Falha não ativa fallback/relay e não diagnostica NAT.

## Encerramento e recuperação — ambos

Convidado: no primeiro terminal, digitar `stop`, aguardar `guest_stopped` e retorno ao PowerShell. Em prompt pendente ou probe, Ctrl+C cancela. Anfitrião: o agente para host e depois echo, aguarda eventosstopped/exit0 e reconfere processos/portas. Supervisor só afeta filhos próprios, conserva plano/evidências e chaves efêmeras desaparecem. Não reutilizar peers/endereço de sessão encerrada. Prazo de reap doSO pode exceder a parada graciosa; nunca declarar cleanup antes de observar encerramento.

Devolver somente JSON final `boundary_probe_passed` ou evento `failed` com código e confirmação de `guest_stopped`/redes diferentes. Não enviar IPs, pins, senhas, tokens, chaves privadas, payloads, áudio ou logs completos. O teste desbloqueia a fronteira/carga sintética nessa rota; Minecraft, áudio, mods, outras topologias e interface continuam com validação separada.
