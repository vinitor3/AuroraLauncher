# ADR-010 — Medir integrações antes de construir rede/relay Aurora

## Contexto

CGNAT/NAT simétrico/UDP bloqueado exigem relay em parte das conexões; tráfego de relay não é gratuito em escala. e4mc e World Host são permissivos e cobrem alvos modernos; playit funciona fora do loader. Essential cobre versões, mas sua licença proíbe copiar e até usar implementação como referência.

## Opções avaliadas

1. integrar mod/serviço existente;
2. reaproveitar módulos/protocolos permitidos;
3. transporte launcher próprio (IPv6/PCP/UPnP/QUIC + proxy TCP);
4. relay Aurora próprio.

## Vantagens e desvantagens

Integração entrega valor e dados cedo, porém depende de terceiros. Transporte próprio cobre toda matriz e exige criptografia/NAT/abuso/operação. Relay próprio contradiz custo zero.

## Riscos

SLA inexistente, IP exposto em P2P, UPnP inseguro, binário terceiro, mudança de termos e promessa universal impossível.

## Decisão

**DECISÃO REGISTRADA: sim.** Pesquisar/implementar uma primeira versão com e4mc e World Host em 1.20.1,upnp ipv6, e playit em conta/túnel do usuário como fallback independente. Não copiar Essential. Somente após métricas decidir módulos permitidos ou “Aurora Direct” no launcher. **ADIAR** relay próprio e não prometer conectividade universal.

## Consequências

Mensagem: “direto grátis quando possível; fallback de terceiro quando a rede exigir relay”. Convite, identidade e sync ficam separados do transporte.

## Reversibilidade

Alta se cada transporte implementar interface de sessão/proxy local.
