# Roadmap do Aurora Smart Launcher

Este roadmap separa claramente o que já está implementado, o que ainda precisa de homologação em jogo e o que é intenção de produto. Ele não representa promessa de prazo.

Atualização de 29/09/2026: base **alpha.14**, Core **1.1.5**, Skins **0.3.1**. Este é o resumo de produto; fases, IDs, gates e planejamento completo ficam no [roadmap mestre](engineering/MASTER_ROADMAP.md). Check de implementação não é check de homologação runtime.

## Agora — estabilização da alpha

- [x] Instâncias isoladas e engine de lançamento em Rust.
- [x] Minecraft Vanilla, Fabric e Forge.
- [x] Java gerenciado conforme a versão do Minecraft.
- [x] Catálogos Modrinth e CurseForge.
- [x] Downloads concorrentes, retomada, hash e gravação atômica.
- [x] Gerenciamento de mods, shaders e resource packs.
- [x] Biblioteca local de skins e prévia 3D.
- [x] Aurora Skins independente, auto-instalado nas nove combinações Fabric/Forge suportadas.
- [x] Aplicação local, favoritos, histórico, importação 64x64/64x32 e personagem 3D na tela principal.
- [x] IPC local autenticado entre launcher e Aurora Core, consumido pelos módulos preservados.
- [ ] Homologar todas as combinações Minecraft/loader em execução real.
- [ ] Concluir a inspeção visual de skin e capa nas versões suportadas.
- [ ] Cobrir fluxos críticos com testes de interface reproduzíveis.
- [x] Conectar um catálogo próprio de skins com resultados visuais paginados e fallback offline.
- [ ] Sincronizar skin ativa, biblioteca e favoritos entre Launcher, jogo e API Aurora.
- [x] Implementar sincronização multiplayer do Skins 0.3.1, preservando grade/cache; a solução atual exige Skins compatível no servidor, conforme [ADR-022](engineering/ADR/ADR-022-skins-server-relay.md). Homologação completa permanece pendente; não anunciar propagação universal sem mod servidor.

## Assistente desktop e módulos mantidos

O Assistente permanece no Aurora Launcher; o mod Companion e sua integração in-game foram retirados. A antiga expansão para HUD/Swing no jogo foi cancelada pela [ADR-018](engineering/ADR/ADR-018-remove-companion-keep-desktop-assistant.md).

- [x] Preservar painel, botão, conversa, Gemini, voz, TTS e atalho desktop `Ctrl+Alt+/`.
- [ ] Repetir homologação desktop com Minecraft fechado, incluindo microfone, fala e falhas dos serviços externos.
- [ ] Homologar Core/Skins nas nove combinações atuais sem Companion.
- [ ] Ampliar a sincronização de skins e capas com fallback local.

## Atualizações de conteúdo — implementado, aguardando homologação

- [x] Detectar mods, shaders e resource packs desatualizados por identidade verificável.
- [x] Atualizar conteúdo preservando Minecraft, loader e dependências obrigatórias, com revisão e rollback.
- [x] Detectar atualizações de modpacks Modrinth e CurseForge.
- [x] Atualizar modpacks Modrinth por versão exata com snapshot transacional.
- [ ] Homologar atualização e rollback em instâncias Minecraft reais; modpacks CurseForge permanecem em detecção/página oficial até o executor transacional específico.

## Depois — distribuição
- [ ] Adicionar cosméticos e emotes ao guarda-roupa.
- [ ] Implementar atualização segura do próprio launcher.
- [ ] Publicar builds assinadas em um canal estável.
- [ ] Criar telemetria opcional e transparente, com privacidade por padrão.

## Diagnóstico e recuperação — alpha.14

- [x] Doctor determinístico local, orientação Gemini opcional, ações restritas com prévia/confirmação/histórico e restauração.
- [x] Pontos manuais com CAS local limitado, prévia, proteção anterior, journal e desfazer.
- [x] Observações verificadas por tentativa, cópias independentes de diagnóstico e aplicação seletiva com confirmação.
- [ ] Homologar os fluxos completos em runtime; preservar limites de mundos/configurações e evidências em [AUR-1167](task-aur-1167-doctor-lab.md).
- [ ] Pontos automáticos gerais antes de updates, retenção e GC seguro. CAS existente não equivale a backup integral nem unificação de executores.

## Aurora Connect V2 — R4, ainda não implementado

- [ ] Pontes TCP e UDP autenticadas por serviço autorizado, convites e estados reais de transporte/jogo/voz.
- [ ] Conectividade direta preferencial e relay gratuito verificado opcional, sem gasto obrigatório/automático; nenhuma infraestrutura própria obrigatória.
- [ ] Compatibilidade individual de Simple Voice Chat e Plasmo Voice, incluindo endereçamento e testes por versão/loader/modo.
- [ ] Manifesto, sincronização consentida, redes reais e liberação gradual após gates. [Especificação e backlog](engineering/AURORA_CONNECT_SPEC.md).

## Fora do escopo atual

- Novo Assistente in-game ou port de novas versões nesta retirada.
- NeoForge.
- Distribuição de conteúdo que viole termos ou licenças de terceiros.
- Captura silenciosa de tela ou envio de dados sem ação explícita do usuário.
