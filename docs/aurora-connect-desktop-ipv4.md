# IPv4 direto no painel experimental — incremento66

**PROTÓTIPO**,14/10/2026, somente com `connect-lab`. Integra as APIs explícitas65 aos quatro perfis do desktop: convite curto TCP, sessão TCP, TCP+UDP curto e sessão TCP+UDP. Não altera os contratos anteriores, não infere modo pelo convite e não tenta outro caminho após falhar.

O backend anuncia `directIpv4` na lista de modos somente quando disponível. O frontend aceita a lista anterior exata ou a lista com esse terceiro modo, recusando duplicados, ausências e valores desconhecidos. O build padrão permanece indisponível e não anuncia IPv4. O endereço começa vazio quando selecionado: o anfitrião informa um IPv4 local autorizado e porta concreta. A revisão mostra esse modo e o endereço antes da aprovação. Destinos privados TCP/UDP continuam loopback e nunca são aceitos do convidado.

Emissão, verificação, Host e Guest usam as entradas65 próprias de cada perfil. Modo, destinatário, serviços, destino e prazos continuam vinculados à revisão/digest; seleção de origem descoberta64 conserva seu guard. O filtro nativo é rechecado antes de I/O; o filtro de interface serve à entrada/revisão, sem substituir o nativo. Não há API nova para endereço público de roteador, firewall, NAT, lookup, relay ou fallback. [Política65 e limites de alcance](aurora-connect-direct-ipv4.md).

## Evidências locais

- Quatro testes nativos sem transporte passaram, incluindo DTO/capability, modos e perfis cruzados, owner/digest/prazos e recusa sem worker/endpoint.
- Três testes opt-in no IPv4 próprio passaram: quatro perfis com TCP/STATUS sintético e UDP de duas origens; schema3 com TCP aberto e UDP4096 após ingresso, oversize e expiração original; falha UDP, recuperação manual após ingresso preservando TCP, revogação e cleanup/rebind. O endereço precisa ser próprio e bindável; variável ausente é falha, não omissão silenciosa.
- **95 testes frontend passaram**, sem ignorados, cobrindo capacidades antigas/novas, quatro perfis, revisão, descoberta64 e ausência de fallback/repetição. Build TypeScript/Vite passou; permanece o aviso anterior de chunk acima de500kB.
- Revisões independentes cruzadas de frontend e nativo e revisão do coordenador sem bloqueador final identificado. [Comandos, logs e demais verificações](task-aur-r4-connect.md#incremento66--ipv4-explícito-no-caller-e-na-interface).

Essas provas usam processos/endpoints próprios e DOM de teste. Não homologam janela Tauri, Minecraft gráfico, áudio, outra máquina, LAN, IPv4 público, CGNAT ou internet móvel. Nenhuma célula Core/Skins foi promovida e nenhuma configuração pessoal foi alterada.

Rollback: encerrar a sessão pelo painel e aguardar cleanup. Um revert revisado da integração66 remove a opção nova e mantém os modos anteriores; resolver qualquer transação Recovery pendente separadamente antes de downgrade do núcleo. Não foi produzido instalador nem publicada versão.
