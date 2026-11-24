# Desenvolvimento do Aurora Core

O Aurora Core é o mod-base obrigatório para as instâncias Fabric/Forge suportadas pelo Aurora. O Launcher escolhe o artefato pela matriz central, valida assinatura e hash, instala Core → Skins e só então inicia o IPC.

O Assistente permanece no Aurora Launcher; o mod Companion e sua integração in-game foram retirados. O Core não oferece página de Assistente nem substitui a ponte removida. Veja [IPC do Core](core-ipc.md).

## APIs públicas

- módulos: metadados SemVer, versão mínima do Core e lifecycle isolado;
- eventos: login, logout, alteração de perfil/skin, conexão do Launcher e configurações;
- sessão: id Aurora, UUID Minecraft, username, estado e scopes, sem credenciais;
- configurações: documento JSON por módulo, schema, migração e backup;
- UI: páginas dinâmicas, layout automático de botões, registro de renderizadores e `AuroraPlayerPreview` version-neutral;
- IPC: envio de mensagens tipadas/extensíveis pelo único socket do Core.

## UI no jogo

O menu padrão de opções recebe o botão **Aurora Options** em uma posição livre calculada depois que os demais mods registram seus controles. A tela central lista páginas dinamicamente, trunca textos dentro da coluna disponível e renderiza o jogador local em 3D com skin atual, modelo classic/slim, camadas externas e movimento suavizado em direção ao mouse. O painel roxo também é desenhado na etapa correta do Forge 1.12.2. Se o render não estiver disponível, somente a prévia é omitida; a tela e o jogo continuam utilizáveis.

O botão de Skins aparece apenas quando `aurora_skins` estiver registrado. Assim, Skins continua um módulo independente e não uma dependência obrigatória do Core.

## Inicialização

1. O Launcher resolve Minecraft, loader e Java.
2. Seleciona Core e Skins pela versão exata e migra Companion gerenciado antigo, com backup/journal e sem tocar em mods incertos.
3. Verifica o Core assinado e instala Core e Skins por troca atômica.
4. Abre um WebSocket loopback com nonce efêmero.
5. Passa somente porta, nonce e loader como propriedades JVM.
6. O Core autentica o `hello`, recebe a projeção pública da sessão e publica eventos aos módulos.

O perfil atual do Launcher ainda usa identidade Minecraft offline; por isso a sessão enviada ao Core tem estado `offline`. A futura autenticação oficial pode fornecer estado `authenticated` sem mudar o contrato e sem transmitir tokens ao jogo.

## Estado de validação

A matriz ativa contém nove alvos Core e nove Skins. Tamanho, SHA-256 e assinatura do Core são verificados por testes Rust; os resultados de rebuild e comparação desta mudança ficam em [task-remove-companion](https://github.com/vinitor3/AuroraLauncher/blob/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs/task-remove-companion.md). Cada JAR inclui `pack.mcmeta` compatível com sua versão do Minecraft. O SLF4J interno é relocacionado para o namespace privado do Core nas versões que o empacotam; testes Rust e o gate de release rejeitam `org/slf4j/**` sem relocação e classes `net/minecraft/**` dentro dos JARs. Os testes Java cobrem compatibilidade SemVer, layout sem colisão, direção/suavização do avatar, isolamento de listeners, rollback de módulo e migração de configuração. Isso não substitui homologação runtime: cada linha permanece **IMPLEMENTADO MAS NÃO HOMOLOGADO** até executar o roteiro da matriz em uma instância limpa.
