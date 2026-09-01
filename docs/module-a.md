# Módulo A — Launcher Core & Engine

## Responsabilidade

O núcleo recebe uma especificação de versão já resolvida, uma identidade de sessão autorizada e uma instância local. Ele:

1. cria e valida a árvore da instância;
2. instala o Aurora Core e o Aurora Companion no diretório `mods` para
   instâncias Fabric/Forge suportadas;
3. valida um Java isolado escolhido para a instância;
4. monta o classpath na ordem informada pelo resolvedor de versões;
5. constrói, inicia e acompanha o processo Java enquanto o launcher estiver aberto;
6. mantém metadados locais de apresentação e permite renomear uma instância
   somente quando ela não está em execução.

Baixar versões, resolver manifests Mojang/mod loaders, login, Firebase, telemetria e atualização pertencem a outros módulos. Separar esses limites impede que o processo de lançamento conheça senhas ou tokens de login.

## Limites de segurança

- IDs de instância são normalizados e não podem conter separadores de caminho.
- Caminhos de JAR e Java precisam existir antes do lançamento.
- Instâncias em execução não podem ser iniciadas novamente, renomeadas ou
  excluídas até o processo terminar.
- A credencial de sessão é um valor opaco e nunca é registrada pelo núcleo.
- Um JWT Firebase não é passado na linha de comando: ela pode ser vista por
  outros processos e logs. A ponte atual passa somente porta efêmera e nonce ao
  Aurora Core, que mantém o socket único usado pelo Companion.
- O produto final deve respeitar as licenças e regras de distribuição aplicáveis ao Minecraft, loaders e mods. O núcleo não valida posse do jogo; essa política deve ser tratada pelo módulo de identidade/distribuição antes do lançamento.

## Integração atual

O comando Tauri `launch_instance` já chama `LauncherEngine::prepare_launch` e depois `spawn`. O resolvedor fornece `VersionLaunchSpec` e a fronteira de sessão fornece `LaunchIdentity`. O contrato ainda não está pronto para distribuição pública: o token de jogo continua sendo scaffolding, e confirmação de entitlement Microsoft/Minecraft deve ocorrer antes do launch.

O próximo hardening deste módulo é limitar HTTP/IPC e tornar a substituição de arquivos realmente transacional. Hoje os downloads validam hashes e usam staging, mas a remoção do destino anterior antes do rename cria uma janela de perda se o commit falhar.

O estado verificável do Companion 0.2.0, inclusive os defeitos dos artefatos
imutáveis e as correções existentes apenas na fonte, fica no
[`dossiê do Companion`](engineering/COMPANION_AUDIT.md).
