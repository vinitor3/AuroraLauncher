# Aurora Connect — primeiro ensaio com dois computadores

Estado: **PRECISA DE TESTE** em dois computadores. O ensaio é somente de bytes TCP/UDP, sem Minecraft, contas ou áudio. Usa o modo manual já testado do laboratório; o convite assinado do incremento27 continua restrito a loopback. Não é instalador, release ou suporte multiplayer homologado.

## O que falta e a ação mínima

Esta conversa executa comandos apenas no Windows atual. Não há acesso remoto autorizado/configurado ao computador convidado. É necessário um segundo **Windows x64**, com uso autorizado, ligado à mesma rede local, e alguém para copiar os arquivos de teste e abrir dois terminais PowerShell nele. Não é preciso instalar Rust, programar, alterar roteador ou enviar credenciais. Na primeira rodada, não usar uma rede diferente.

O anfitrião é o PC atual: o agente pode preparar/executar todo esse lado. O convidado é o segundo PC. A pasta de ensaio contém `aurora-connect-lab.exe` e este roteiro. A cópia pode ser feita por um meio privado já disponível, por exemplo pendrive, para uma pasta nova `C:/AuroraConnectLab`. Não sobrescrever uma pasta existente; nesse caso usar outro nome e ajustar `Set-Location` nos **dois** blocos do convidado para a mesma pasta. Ambos conferem o hash antes de executar.

Binário debug Windows x64 verificado localmente, SHA-256:

`2cc277150e942dab68ab8b2037a4b1791a8ed1e06a7cc2f6af8bc411be36caac`

Fonte nesta worktree: `apps/connect-lab`, branch `codex/aur-r4-connect`; dependências fixadas em seu `Cargo.lock`. Esse hash identifica o artefato testado, não uma assinatura de distribuição. Se o hash divergir, não executar: conferir a cópia e o artefato revisado. Não baixar um substituto aleatório.

## Anfitrião — executado neste PC pelo agente

1. Conferir hash, portas TCP42001/UDP42000/42002 livres e selecionar o IPv4 local da interface usada na mesma rede do convidado. Nenhum processo alheio é encerrado e nenhuma regra de firewall é alterada.
2. Abrir endpoint sintético e host com prazo600s, em terminais separados. `<IPv4-local>` é preenchido após a consulta da interface, nunca enviado literalmente:

```powershell
$lab = 'C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect/apps/connect-lab/target/debug/aurora-connect-lab.exe'
& $lab echo 127.0.0.1:42001 127.0.0.1:42002 600
```

```powershell
$lab = 'C:/Users/usuario/Desktop/AuroraLauncher-worktrees/codex-aur-r4-connect/apps/connect-lab/target/debug/aurora-connect-lab.exe'
& $lab host <IPv4-local>:42000 127.0.0.1:42001 127.0.0.1:42002 600
```

3. Entregar privadamente ao convidado o IPv4 e o `peer` público de `host_identity`. Neste modo, `session` é igual ao `peer`; conferir essa igualdade na saída real. Chave privada não sai do processo.
4. Receber o `peer` de `guest_identity`, conferir64hex, inseri-lo no stdin do host e aguardar `host_ready`. Só então autorizar o convidado a digitar `ready`.

## Convidado — segundo PC, primeiro PowerShell

Copiar a pasta nova, abrir PowerShell e executar o bloco inteiro abaixo, incluindo `& {` e `}`. Erro interrompe o bloco antes da execução; a política de erro fica somente nesse escopo. Os dois prompts recebem apenas o endereço local e o peer **público** do anfitrião, fornecidos na etapa anterior:

```powershell
& {
$ErrorActionPreference = 'Stop'
Set-Location C:/AuroraConnectLab
$lab = (Resolve-Path ./aurora-connect-lab.exe).Path
$expected = '2cc277150e942dab68ab8b2037a4b1791a8ed1e06a7cc2f6af8bc411be36caac'
if ((Get-FileHash -LiteralPath $lab -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'LAB_HASH_MISMATCH: nao executar' }
$hostAddress = Read-Host 'IPv4 local do anfitriao, sem porta'
$hostPeer = Read-Host 'Peer publico do anfitriao, 64 caracteres hexadecimais'
if ($hostPeer -cnotmatch '^[a-f0-9]{64}$') { throw 'LAB_PEER_INVALID' }
$address = $null
if (-not [System.Net.IPAddress]::TryParse($hostAddress, [ref]$address) -or $address.AddressFamily -ne [System.Net.Sockets.AddressFamily]::InterNetwork) { throw 'LAB_IPV4_INVALID' }
& $lab guest $hostPeer ($address.ToString() + ':42000') $hostPeer 127.0.0.1:42010 127.0.0.1:42011 600
}
```

Enviar o `peer` público mostrado em `guest_identity` ao anfitrião pelo canal privado combinado. Depois de o anfitrião confirmar `host_ready`, digitar **`ready`** e Enter neste terminal. Esperar `guest_ready`. Se aparecer pedido do Windows para mudar o firewall, cancelar e informar que houve o pedido; qualquer regra temporária precisa de plano específico e recuperação, não de liberação ampla improvisada.

## Convidado — segundo PowerShell

Após `guest_ready`, executar este segundo bloco inteiro:

```powershell
& {
$ErrorActionPreference = 'Stop'
Set-Location C:/AuroraConnectLab
$lab = (Resolve-Path ./aurora-connect-lab.exe).Path
$expected = '2cc277150e942dab68ab8b2037a4b1791a8ed1e06a7cc2f6af8bc411be36caac'
if ((Get-FileHash -LiteralPath $lab -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'LAB_HASH_MISMATCH: nao executar' }
& $lab probe 127.0.0.1:42010 127.0.0.1:42011
}
```

Esperado: `probe_passed`,1.048.576B TCP por direção, dez datagramas íntegros em duas origens. `guest_ready` sozinho não prova o encaminhamento. Timeout não identifica NAT/CGNAT; registrar o código e encerrar, sem trocar de serviço ou configurar opção paga.

## Encerramento e recuperação

No primeiro terminal do convidado: digitar **`stop`**, aguardar `guest_stopped` e retorno ao PowerShell. No anfitrião: `stop` no host, aguardar `host_stopped`, depois `stop` no echo e aguardar seu encerramento. Se estiver aguardando um pin/`ready`, cancelar com Ctrl+C; não esperar um evento stopped que só existe após iniciar a sessão. O prazo limita uma sessão iniciada a600s. Nenhum processo alheio deve ser encerrado.

As chaves efêmeras são descartadas ao sair. Não há configuração Minecraft/mod, arquivo pessoal, regra de firewall ou mapeamento de roteador a restaurar. A pasta nova pode permanecer para inspeção; não executar exclusão recursiva ou limpeza automática. Reiniciar exige novos peers públicos e nova autorização do host.

Devolver somente: PASS ou código fixo da falha, Windows x64 de cada máquina, mesma rede local ou rede diferente, contagens/durações do `probe_passed`, confirmação do encerramento nos dois lados e eventual pedido de firewall. Não devolver linhas de identidade, IPs, convites, senhas, tokens, áudio ou logs completos. O resultado desbloqueia a avaliação do transporte direto em LAN; ainda não valida internet/NAT/relay, Minecraft, mods de voz, clientes mistos ou UI.
