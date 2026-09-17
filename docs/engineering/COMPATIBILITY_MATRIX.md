# Compatibility Matrix

Atualizado em 2026-09-02. `BUILD` significa JAR compilado e inspecionado; `RUNTIME` exige o roteiro completo em uma instância real. Core 1.1.2, Skins 0.2.1 e Companion 0.2.3 formam a matriz embarcada no Launcher 0.1.1-alpha.6.

| Minecraft | Loader | Java | Core build | Skins build | Companion build | Runtime do conjunto | Estado |
| --- | --- | ---: | :---: | :---: | :---: | :---: | --- |
| 1.12.2 | Forge 14.23.5.2860 | 8 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.16.5 | Fabric 0.16.14 | 8 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.16.5 | Forge 36.2.42 | 8 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.19.2 | Fabric 0.16.14 | 17 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.19.2 | Forge 43.4.16 | 17 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.20.1 | Fabric 0.16.14 | 17 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.20.1 | Forge 47.4.23 | 17 | sim | sim | sim | Cave Horror: launch/menu/IPC/skin/catálogo PASS; mundo PRECISA DE TESTE | PARCIAL |
| 1.21.1 | Fabric 0.16.14 | 21 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |
| 1.21.1 | Forge 52.1.12 | 21 | sim | sim | sim | PRECISA DE TESTE | IMPLEMENTADO MAS NÃO HOMOLOGADO |

Evidências runtime anteriores do Companion 1.12.2 Forge e 1.20.1 Fabric não homologam automaticamente os novos Core/Skins. O conjunto precisa ser reexecutado porque a aparência deixou de pertencer ao Companion e passou para o módulo Aurora Skins.

Em 1º de setembro de 2026, o Cave Horror Project 1.20.1 Forge 47.4.21 iniciou
com Java 17 usando exatamente os JARs embarcados Core 1.1.2, Skins 0.2.1 e
Companion 0.2.3. Não houve conflito JPMS de SLF4J/Minecraft nem rejeição de
resource pack; os três módulos registraram, o IPC conectou, a skin apareceu em
pé, o botão permaneceu sob o avatar no FancyMenu e abriu o catálogo online. Um
mundo/servidor não foi aberto nessa execução, portanto a linha continua parcial.

## Instalação pelo Launcher

Atualização de 16/09/2026: **Skins 0.2.2** tem nove JARs compilados (1.12.2 Forge e Fabric/Forge 1.16.5, 1.19.2, 1.20.1, 1.21.1), busca mc-api/MCStat, debounce e prévia do resultado. Os hashes estão em `releases/skins/0.2.2/manifest.json`; o código do launcher passou a embarcar essa matriz. **IMPLEMENTADO MAS NÃO HOMOLOGADO**: nenhum runtime/gameplay de 0.2.2 está comprovado. A tabela e a evidência Cave Horror acima são históricas do conjunto 0.2.1/alpha.5, não homologam 0.2.2. Não foi substituído o instalador alpha.5.

Para cada instância Fabric/Forge suportada, o Launcher instala por troca atômica:

1. Aurora Core 1.1.2, após validar tamanho, SHA-256 e assinatura Ed25519;
2. Aurora Skins 0.2.1;
3. Aurora Companion 0.2.3.

O release preflight recompila as 27 combinações e compara o conteúdo normalizado dos JARs de staging com os artefatos embarcados. A normalização ignora apenas o namespace aleatório de injeção gerado pelo Architectury e metadados ZIP; qualquer outra divergência bloqueia a criação do instalador. O mesmo gate exige `pack.mcmeta` em todos os módulos e abre cada Core para rejeitar `org/slf4j/**` sem relocação ou qualquer `net/minecraft/**`, evitando os conflitos e avisos observados no Cave Horror.

## Compatibilidade do launcher

- Guarda-roupa, correção de 17/09/2026: recuperação da skin local por PNG,
  seleção equipada persistente e espera da recuperação antes de iniciar o jogo.
  O launch prioriza o modelo Slim/Clássico equipado local, mesmo quando o perfil
  remoto está desatualizado. **IMPLEMENTADO MAS NÃO HOMOLOGADO** no executável
  empacotado; fixture visual e testes do IPC simulado não homologam gameplay.
  Nenhum JAR ou instalador foi alterado. Ver `../task-wardrobe-recovery.md`.

- Inicialização Windows sem console, incluindo probes Java/Forge/jogo:
  **IMPLEMENTADO MAS NÃO HOMOLOGADO** na fonte posterior à alpha.6. Testes de
  subprocesso confirmam console ausente e stdout/stderr preservados; executar
  a próxima build empacotada e iniciar Minecraft ainda **PRECISA DE TESTE**.
  Sessão restaurada usa tela animada sem antecipar o login; detalhes em
  `../startup-session.md`.

- Minecraft 1.16.5 em sessão offline: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. O
  launcher isola somente o endpoint de privilégios do `authlib 2.1.28`, fazendo
  o cliente usar o fallback `OfflineSocialInteractions` e preservar Multiplayer
  LAN/offline. Há teste unitário do escopo por versão e prova direta com o
  `authlib` da instância; a confirmação visual do botão ainda **PRECISA DE
  TESTE** em uma nova build desktop.

## Roteiro runtime obrigatório

Skins **0.2.3** substitui 0.2.2 na próxima build do launcher: a prévia guarda somente o PNG e não altera metadados da biblioteca/skin ativa, inclusive quando a textura é idêntica e o modelo é diferente. 0.2.2 permanece imutável como histórico. Estado das nove combinações: **IMPLEMENTADO MAS NÃO HOMOLOGADO**, até repetição do roteiro abaixo.

1. Instância limpa e hashes de instalador/JAR.
2. Java/loader corretos e launch sem argumento sensível.
3. Handshake IPC e rejeição de nonce inválido.
4. Core → Skins → Companion; somente uma conexão WebSocket.
5. Options → Aurora Options; botão sem sobreposição em diferentes GUI scales.
6. Painel roxo, textos truncados sem colisão e abrir/fechar repetido por botão/ESC.
7. Avatar 3D na tela principal e na Central Aurora, classic/slim, duas camadas e fallback.
8. Mouse para cima/baixo e esquerda/direita com cabeça/corpo na direção correta e suavização.
9. Importar/aplicar PNG 64x64 classic, slim e 64x32 convertida; reiniciar e confirmar persistência.
10. Mundo não pausa; teclado/mouse voltam ao jogo sem travar.
11. Em 1.16.5, Multiplayer não deve ser bloqueado pela conta Microsoft fictícia; servidor `online-mode` continua exigindo autenticação oficial.
12. Falha de módulo, skin HTTPS, IPC e encerramento sem impedir o Minecraft de iniciar.

## Estratégia

- Referência moderna: Fabric/Forge 1.20.1.
- 1.21.1 usa Java 21 e adaptadores próprios de `PlayerSkin`.
- 1.16.5 preserva bytecode Java 8 e identidade de launch `legacy`/offline.
- 1.12.2 permanece em trilho Forge legado Java 8/LWJGL2.
- NeoForge: fora do escopo por decisão atual.
