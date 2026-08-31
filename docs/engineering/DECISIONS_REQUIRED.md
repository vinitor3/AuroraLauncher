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

**Decisão registrada: sim.**

**Escopo aprovado para a primeira versão:** usar conexão direta e providers externos opcionais (e4mc/World Host/playit/BYO), sem relay Aurora e sem garantia universal.

**Recomendação:** sim. Isso mantém custo obrigatório zero e permite medir taxa direta, latência e demanda. Usuário escolhe/autoriza o provider; nenhum serviço externo vira dependência oculta.

**Se não:** a feature deve esperar orçamento/operador para relay, proteção antiabuso, privacidade, monitoramento e suporte.
