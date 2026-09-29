# Decisões requeridas do responsável

Somente decisões que mudam contrato público ou direitos permanecem aqui. O restante tem padrão técnico recomendado nos ADRs.

## D-001 — licença do monorepo

**Decisão necessária antes de integrar AUR-R0-002:** licenciar todo o código autoral do Aurora (Rust, TypeScript, Java, scripts e documentação indicada) sob GPL-3.0-only, mantendo assets/marcas e componentes de terceiros com avisos próprios?

**Recomendação:** sim. É coerente com a intenção de manter o Aurora gratuito/open source e com o manifesto Rust atual. Excluir explicitamente segredos, marcas de terceiros, Minecraft e artefatos que o projeto não pode relicenciar. Incluir texto integral, SPDX, política de contribuições e inventário de terceiros.

**Se não:** escolher uma licença OSI compatível antes de aceitar contribuição/copiar código; a Wave 0 fica bloqueada.

## D-002 — autenticação para distribuição pública

**Decisão registrada: não, por enquanto.**

**Condição para uma futura distribuição pública:** o Aurora deverá exigir login Microsoft/Xbox/Minecraft e entitlement válido para download/launch público.

**Recomendação:** sim. Firebase continua como identidade social/Aurora, mas não substitui propriedade do jogo. Modo offline fica restrito a instalações obtidas legitimamente e documentado como modo degradado.

**Se não:** manter o produto como protótipo de desenvolvimento privado e não publicar fluxo que use token fictício ou baixe conteúdo do jogo.

## D-003 — estratégia de fallback multiplayer

**Decisão de produto revisada em 29/09/2026 por requisito explícito Aurora Connect V2.** A decisão anterior aceitava direto e providers opcionais. A revisão esclarece relay gratuito permitido, transporte TCP/UDP e compatibilidade individual com Simple Voice Chat/Plasmo Voice, sem criar voz própria.

**Escopo vigente:** direto preferido; relay gratuito só após verificar termos, integração, quotas, autenticação e uso pretendido. Nenhuma dependência de produção é aprovada apenas por funcionar em teste. Proibidos gasto obrigatório/automático, billing ativado por esta tarefa e excedente cobrável sem bloqueio efetivo. Relay próprio obrigatório permanece adiado; o usuário não precisa hospedá-lo para cumprir o MVP. Sem caminho permitido, falha clara, sem fallback pago silencioso ou promessa universal.

**Proposta técnica, ainda não decisão aceita:** avaliar Iroh primeiro; comparar Quinn com conectividade compatível e manter e4mc/World Host como PoCs/referências conforme TCP/UDP/voz/custo. Processo auxiliar Go só por vantagem concreta. A proposta e as pendências estão na [ADR sem número](ADR/PROPOSED-aurora-connect-transport.md); termos e aprovações por ambiente em [dependências](AURORA_CONNECT_DEPENDENCIES.md).

**Decisões pendentes:** versão/biblioteca após PoC, sinalização de produto, eventual relay gratuito sustentável de distribuição, versões/artefatos dos mods e adaptação por participante. Ausência de relay aprovado preserva modo direto limitado; não implica contratar infraestrutura. A revisão documental não habilita feature, serviço ou faturamento.
