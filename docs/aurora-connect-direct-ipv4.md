# Convites com IPv4 inicial explícito — incremento65

Data: 14/10/2026. **PROTÓTIPO**, biblioteca experimental; integração de interface requer etapa própria. O modo `directIpv4` é uma escolha explícita do caller, não é inferido do convite e não substitui os modos loopback ou IPv6 externo.

## Contrato de endereço e custo

O endereço inicial precisa ser IPv4 literal canônico, com porta diferente de zero. RFC1918 é permitido para uso em rede alcançável. A política conservadora recusa `0/8`, `127/8`, `100.64/10`, `169.254/16`, `224/3`, `192.0.0/24`, `192.0.2/24`, `192.31.196/24`, `192.52.193/24`, `192.88.99/24`, `192.175.48/24`, `198.18/15`, `198.51.100/24` e `203.0.113/24`, inclusive as exceções internas desses blocos. IPv6, endereços mapeados em IPv6, DNS e porta zero são recusados.

A escolha desses blocos usa o [registro oficial IANA](https://www.iana.org/assignments/iana-ipv4-special-registry), consultado em14/10/2026, com atualização indicada em09/10/2025. É política própria conservadora; não é um classificador universal de roteabilidade. Não identifica broadcasts dirigidos de qualquer sub-rede, NAT, CGNAT ou alcance. Permissão sintática de um endereço não autoriza testar equipamentos de terceiros.

O Host exige que o bind seja exatamente o endereço assinado, efetivamente pertencente ao computador. Esta etapa não separa endereço público de roteador e bind local, não configura redirecionamento de portas, hole punching, firewall ou descoberta. O Guest usa transporte em `0.0.0.0:0`; listeners de aplicação e destinos do Host permanecem loopback. O destino privado do jogo/UDP nunca vem do convidado.

Iroh1.3.0 e locks atuais permanecem; nenhum pacote, serviço externo, conta, crédito ou cobrança é acrescentado. Relay, lookup e portmapper permanecem desligados. O filtro regula o endereço inicial, sem prometer fixar todos os caminhos internos do Iroh ou migração homologada.

## Autoridade preservada

Os quatro perfis têm APIs explícitas: legado schema1, fragmentado schema1, sessãoTCP schema2 e sessãoTCP+UDP schema3. `Invitation` e `SessionInvitation` acrescentam métodos `issue_*direct_ipv4*` e `verify_*direct_ipv4`; Host/Guest acrescentam `start_invited_*direct_ipv4`/`connect_invited_*direct_ipv4`. Não há alteração de schema, domínio de assinatura, grants, prova de posse ou conteúdo canônico dos perfis anteriores.

Pin independente do Host, destinatário preso à chave TLS, serviço autorizado, consumo único antes do ACK, revogação e relógios originais continuam obrigatórios. APIs loopback/IPv6 e APIs de outro perfil recusam o objeto incompatível antes de iniciar I/O, mesmo se o objeto já foi verificado por outra entrada. Bind diferente, registro divergente ou chave convidada errada também falham antes do transporte.

Schema1 mantém TTL máximo600s. Schema2/3 mantêm ingresso≤600s e sessão≤3600s, sem renovação. Schema3 conserva originalUDP4096B, framesV2≤1264B e retorno16384B/s/20frames/s; não altera os limites de áudio. CLI, kits históricos, Node e desktop anteriores não ganham implicitamente esse modo.

## Provas locais e limites

O teste `apps/connect-lab/tests/direct_ipv4.rs` contém verificações sem runtime/rede e ensaios opt-in no IPv4 próprio explicitamente selecionado por `AURORA_CONNECT_TEST_IPV4`. O teste reserva o bind local antes de conectar; ausência da variável ou endereço não bindável é falha, nunca sucesso por omissão. Endereço real, peers e convites não são registrados em Git/log compartilhável.

Resultados executados em14/10/2026: cinco testes sem transporte passaram; os três ensaios opt-in passaram no endereço próprio deste PC. Os quatro perfis transportaram TCP65536B por direção cada; os três perfis UDP usaram duas origens, com512B no legado e4096B nos perfis fragmentados. Outro ensaio conservou TCP e duas origens UDP após o prazo de ingresso, encerrou no prazo original da sessão e recusou4097B antes do endpoint. Replay e revogação foram correlacionados com contadores do Host e ausência de novos destinos/payload. Cleanup e rebind dos sockets passaram. Os endereços não foram incluídos no registro compartilhável.

Regressão completa da biblioteca: **236 PASS/7 ignorados** no padrão e **246 PASS/8 ignorados** com a fixture `local-relay-lab`; três desses ignorados são os opt-in IPv4 executados separadamente. A fixture de relay permanece exclusivamente local e não representa serviço público. Comandos completos e logs estão no [task](task-aur-r4-connect.md#incremento65--convites-e-transporte-com-ipv4-inicial-explícito).

Tráfego autenticado em endereço próprio não representa LAN entre máquinas, IPv4 público, CGNAT, internet móvel, áudio, Minecraft ou estado de mundo. As evidências IPv6 externas anteriores permanecem separadas. Para testar outro computador será preciso kit novo revisado, endereços atuais, novos peers/convites e autorização apropriada; os comandos antigos não são reutilizados.

Rollback: encerrar Guest/Host e aguardar liberação de listeners/associações. Revert local revisado somente desta extensão restaura as APIs anteriores; nenhuma configuração pessoal precisa ser desfeita. Não há publicação, instalador ou liberação de produto.
