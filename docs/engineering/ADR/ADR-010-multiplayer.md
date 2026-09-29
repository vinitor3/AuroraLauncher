# ADR-010 — Medir integrações antes de construir rede/relay Aurora

## Revisão de escopo — Aurora Connect V2, 29/09/2026

O registro abaixo permanece como histórico da escolha inicial, não como aprovação atual dos serviços citados. O novo requisito explícito prioriza avaliar Iroh, comparando Quinn e integrações existentes, com ponte TCP **e UDP**, incluindo compatibilidade com Simple Voice Chat e Plasmo Voice. [ADR de transporte proposta, ainda sem número](PROPOSED-aurora-connect-transport.md). A escolha de tecnologia continua pendente de aceite/evidência.

A política vigente prefere caminho direto e permite relay gratuito verificado para o uso pretendido. Proíbe gasto obrigatório/automático; nenhum relay de produção é aprovado por esta revisão. Infraestrutura própria obrigatória permanece adiada. Não exigir conta/túnel externo, VPN, driver ou portas manuais na experiência final suportada. e4mc/World Host/playit precisam de reavaliação de termos, licença, TCP/UDP e endereçamento de voz; referência histórica não autoriza bundling nem presume suporte à matriz. [Especificação](../AURORA_CONNECT_SPEC.md) e [dependências](../AURORA_CONNECT_DEPENDENCIES.md).

As afirmações históricas sobre custo, cobertura de toda a matriz e necessidade de relay por tipo de NAT devem ser lidas como motivação da pesquisa, não resultados. Tipo de NAT/CGNAT ou timeout isolado não prova inviabilidade direta; observação do caminho e testes são necessários. A revisão V2 prevalece nos pontos conflitantes e não altera a retirada do Companion.

## Registro original

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
