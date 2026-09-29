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

**Decisão anterior substituída pela instrução explícita Aurora Connect V2 de 29/09/2026.** O ID é preservado para rastreabilidade. O registro antigo aprovava direto e providers externos opcionais, sem relay Aurora. Ele não autoriza hoje um provider por nome nem impede a avaliação de relay gratuito.

**Política vigente:** priorizar conexão direta e permitir relay gratuito somente após verificar gratuidade, integração e autorização para o uso pretendido. Não contratar, habilitar billing, usar excedente cobrável sem bloqueio efetivo comprovado, depender de VPS própria, promoção temporária ou exigir que o jogador opere relay. Alerta de orçamento e contador local não bastam. Infraestrutura Aurora própria obrigatória permanece adiada.

**Aplicação:** laboratório e distribuição pública precisam de avaliação separada. Documentar operador, finalidade, autenticação, licença, termos, quotas, regiões, limitações, risco de cobrança, indisponibilidade, fonte oficial e data. A disponibilidade técnica de um relay não comprova autorização de produção. Se não houver relay gratuito adequado, preservar direto e informar a limitação; sem qualquer caminho permitido, falhar claramente. Não há garantia universal em NAT/CGNAT.

**Decisões técnicas pendentes, sem nova autorização de gasto:** fixar biblioteca/versão após auditoria, validar transporte TCP e UDP, endereçamento por participante dos mods e aprovar cada dependência externa. Avaliar Iroh primeiro e comparar alternativas sem implementá-las todas. Uma conexão pode iniciar por relay gratuito já autorizado enquanto tenta caminho direto; medir migração e impacto no áudio antes de prometer continuidade.

O alcance inclui Simple Voice Chat e Plasmo Voice separadamente, sem criar voz própria. e4mc/World Host são referências comparativas, não fornecedores automaticamente homologados. [Especificação Connect V2](AURORA_CONNECT_SPEC.md), [ADR candidata](ADR/PROPOSAL-aurora-connect.md) e [matriz de dependências](AURORA_CONNECT_DEPENDENCIES.md) registram condições e pendências. D-001, D-002 e os demais gates do roadmap continuam válidos.
