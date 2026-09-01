# Compatibility Matrix

Atualizado em 2026-08-31 após inspeção dos JARs 0.2.0 e dos logs locais mais
recentes. `BUILD` significa somente que a fonte/artefato é legível e compila;
`RUNTIME COMPLETO` exige todo o roteiro abaixo. Aurora Core 1.0.0 compila nos
nove alvos, mas nenhuma combinação Core + Companion 0.2.0 concluiu o roteiro.

| Minecraft | Loader | Java | Core build | Companion 0.2 build | Evidência 0.2 observada | Runtime completo 0.2 |
| --- | --- | ---: | :---: | :---: | --- | :---: |
| 1.12.2 | Forge | 8 | sim | sim | PARCIAL: Core e módulo carregaram, aparência/atalho apareceram e o mundo encerrou; handshake e fluxo do Assistente não foram provados | não |
| 1.16.5 | Fabric | 8 | sim | sim | RISCO: um pack recusou Fabric Loader 0.13.3; outro iniciou, mas o Companion executou antes do Core e não anexou o módulo | não |
| 1.16.5 | Forge | 8 | sim | sim | RISCO: JAR imutável declara Forge `[47,)` em vez de `[36,)` | não |
| 1.19.2 | Fabric | 17 | sim | sim | RISCO: Core conectou, mas o Companion executou antes do Core e não anexou o módulo | não |
| 1.19.2 | Forge | 17 | sim | sim | RISCO: JAR imutável declara Forge `[47,)` em vez de `[43,)` | não |
| 1.20.1 | Fabric | 17 | sim | sim | RISCO: Core conectou, mas o Companion executou antes do Core e não anexou o módulo | não |
| 1.20.1 | Forge | 17 | sim | sim | NÃO VERIFICADO | não |
| 1.21.1 | Fabric | 21 | sim | sim | RISCO: mesma corrida de inicialização; adaptador também não localizou o perfil para aplicar aparência | não |
| 1.21.1 | Forge | 21 | sim | sim | NÃO VERIFICADO | não |

As homologações anteriormente citadas para Forge 1.12.2 e Fabric 1.20.1 são do
Companion 0.1.0, quando cada JAR possuía cliente WebSocket próprio. Elas são
evidência histórica, mas não homologam o protocolo único do Core nem o
Companion 0.2.0. A fonte posterior aos JARs 0.2.0 corrige a faixa Forge e tenta
anexar novamente ao Core; permanece **PRECISA DE TESTE** até ser publicada com
versão nova e executada.

## Compatibilidade do launcher

- Minecraft 1.16.5 em sessão offline: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. O
  launcher isola somente o endpoint de privilégios do `authlib 2.1.28`, fazendo
  o cliente usar o fallback `OfflineSocialInteractions` e preservar Multiplayer
  LAN/offline. Há teste unitário do escopo por versão e prova direta com o
  `authlib` da instância; a confirmação visual do botão ainda **PRECISA DE
  TESTE** em uma nova build desktop.

## Roteiro runtime obrigatório

1. Instância limpa e hashes de instalador/JAR.
2. Java/loader corretos e launch sem argumento sensível.
3. Handshake IPC e rejeição de nonce inválido.
4. Core instalado antes do Companion; somente uma conexão WebSocket.
5. Options → Aurora Options; abrir/fechar repetido; ESC.
6. Avatar 3D com classic/slim, camadas externas, mouse e fallback de render.
7. Mundo não pausa; teclado/mouse voltam ao jogo sem travar.
8. Texto, voz, resposta, legenda e mute/TTS.
9. Screenshot somente após confirmação e sem telemetria.
10. Skin 64x64 classic, slim, 64x32 convertida e capa, com inspeção visual.
11. Falha de módulo/IPC/Worker e encerramento do Minecraft/launcher.
12. Migração/backup de configuração e evidência redigida anexada à execução.

## Estratégia de portes

- Referência: Core/Companion Fabric 1.20.1.
- Segundo alvo: Forge 1.20.1.
- Depois: 1.19.2, 1.16.5, 1.21.1 conforme adaptadores reais.
- Último: Forge 1.12.2, preservando Java 8/LWJGL2 e sem bloquear o moderno.
- NeoForge: fora do escopo por decisão atual.
