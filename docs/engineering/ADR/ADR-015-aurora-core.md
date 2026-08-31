# ADR-015 — Aurora Core como fundação modular in-game

## Contexto

Sessão, IPC, configuração e telas estavam distribuídos entre Launcher e Companion. Novos recursos tenderiam a criar conexões paralelas, dependências rígidas e implementações repetidas nas nove combinações Minecraft/loader.

## Opções avaliadas

1. manter cada mod autônomo, com sessão e IPC próprios;
2. transformar Skins ou Companion no mod-base;
3. criar um Aurora Core pequeno, estável e obrigatório, deixando recursos como módulos.

## Decisão

Adotar a opção 3. Aurora Core oferece APIs Java 8 sem tipos Minecraft, runtime comum e adaptadores finos por versão/loader. O Core possui o único WebSocket do jogo, recebe somente uma projeção pública da sessão e expõe event bus, configuração e páginas dinâmicas. Companion e Skins permanecem módulos independentes.

O Launcher instala o Core antes dos demais módulos. A seleção vem de um manifesto central com versão de Minecraft/loader/Java, SHA-256 e assinatura Ed25519; a chave confiável é fixada no binário. Releases publicadas são imutáveis.

## Consequências

- módulos não repetem sessão, IPC ou menu;
- falhas são isoladas e registros parciais são desfeitos;
- 1.12.2 permanece adaptador legado separado, mas usa a mesma API Java 8;
- mudanças incompatíveis exigem nova major do Core;
- build válido não equivale a homologação runtime por combinação.

## Estado

Aceita e implementada em 2026-08-31. Homologação runtime permanece pendente.
