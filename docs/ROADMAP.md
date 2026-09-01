# Roadmap do Aurora Smart Launcher

Este roadmap separa claramente o que já está implementado, o que ainda precisa de homologação em jogo e o que é intenção de produto. Ele não representa promessa de prazo.

## Agora — estabilização da alpha

- [x] Instâncias isoladas e engine de lançamento em Rust.
- [x] Minecraft Vanilla, Fabric e Forge.
- [x] Java gerenciado conforme a versão do Minecraft.
- [x] Catálogos Modrinth e CurseForge.
- [x] Downloads concorrentes, retomada, hash e gravação atômica.
- [x] Gerenciamento de mods, shaders e resource packs.
- [x] Biblioteca local de skins e prévia 3D.
- [x] IPC local autenticado entre launcher e Aurora Core, compartilhado pelo Companion.
- [ ] Homologar todas as combinações Minecraft/loader em execução real.
- [ ] Concluir a inspeção visual de skin e capa nas versões suportadas.
- [ ] Cobrir fluxos críticos com testes de interface reproduzíveis.

## Próximo — Companion nativo

- [ ] Substituir a janela Swing externa por uma tela/HUD nativos do Minecraft.
- [ ] Manter o mundo jogável enquanto o painel estiver aberto.
- [ ] Adaptar a interface para Forge 1.12.2 e Fabric/Forge 1.16.5–1.21.1.
- [ ] Finalizar entrada por voz, resposta, legendas e controle de áudio dentro do jogo.
- [ ] Ampliar a sincronização de skins e capas com fallback local.

## Depois — conteúdo e distribuição

- [ ] Detectar mods desatualizados.
- [ ] Atualizar mods preservando Minecraft, loader e dependências.
- [ ] Detectar e atualizar versões de modpacks.
- [ ] Adicionar cosméticos e emotes ao guarda-roupa.
- [ ] Implementar atualização segura do próprio launcher.
- [ ] Publicar builds assinadas em um canal estável.
- [ ] Criar telemetria opcional e transparente, com privacidade por padrão.

## Fora do escopo atual

- NeoForge.
- Distribuição de conteúdo que viole termos ou licenças de terceiros.
- Captura silenciosa de tela ou envio de dados sem ação explícita do usuário.
