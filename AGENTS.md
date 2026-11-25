# Aurora Smart Launcher

Trabalhe numa branch curta de integração em uma worktree exclusiva. Consulte `npm run release:status` antes de integrar outras entregas. Preserve a versão cumulativa mais recente, alterações locais e histórico. Coordene um responsável por arquivo compartilhado, em especial `App.tsx`, `commands.rs` e `edge-proxy/src/index.ts`.

## Produto

- Desktop React/Tauri, engine Rust, Core e Skins em Java. O Worker atende os catálogos e as aparências. Firebase fornece identidade Aurora.
- O diagnóstico é local e o usuário pode copiar um resumo para um serviço externo. A beta experimental e a integração Microsoft futura estão registradas na [ADR-036](docs/engineering/ADR/ADR-036-beta-diagnostics.md).
- Java e loaders compatíveis com o jogo e os módulos Aurora são decisões separadas. Consulte a [matriz ativa](scripts/active-mod-matrix.json) e preserve os contratos de grade, cache, multiplayer, migração e assinaturas. 1.12.2 tem caminho legado próprio. NeoForge exige port e decisão de produto.
- O laboratório Connect usa permissões por feature, consentimento e recibos específicos. Preserve os limites de autenticação, relay, voz e recuperação. Consulte a [especificação](docs/engineering/AURORA_CONNECT_SPEC.md) e os contratos do componente antes de alterar os fluxos. O serviço deve operar dentro do orçamento já autorizado.
- Integre mudanças visuais pelos tokens e IDs semânticos existentes. Atualize os contratos TS/Rust, migração, editor e testes se mudar o esquema do tema. [Integração visual](docs/theme-integration.md).

## Verificação

Teste as áreas alteradas. Para a beta completa, valide frontend, testes Rust, Worker, Functions e matriz Gradle. Verifique formatação e Clippy. Registre o que compilou, o que rodou em Minecraft, os alvos afetados e os limites dos testes. Use a recuperação confirmada para mutações de instância. Não confunda compilação com homologação.

- Preserve caminhos confinados, downloads verificados e substituição recuperável. Não copie mundos, dados pessoais, logs privados ou credenciais para Git.
- IPC do jogo usa loopback, porta efêmera e handshake. Nunca envie JWT, senha ou chave de serviço à JVM. Valide dados externos e valide autorização em cada operação privilegiada.
- A retirada do Companion legado exige proveniência, backup e journal recuperável. Preserve as regras da recuperação manual e das cópias de diagnóstico.
- Para material de terceiros, siga os termos de redistribuição e o fluxo autorizado do provedor. O instalador distribui o software Aurora.
- Após commit, gere uma versão inédita com `npm run release:package`. O wrapper chama o build oficial, verifica o checkout e a matriz e registra SHA-256 e manifesto. Preserve tags e artefatos anteriores.

O [guia rápido](docs/README.md), a [matriz](docs/engineering/COMPATIBILITY_MATRIX.md) e as [decisões](docs/engineering/ADR/README.md) são as referências atuais. O [arquivo pré-beta](https://github.com/vinitor3/AuroraLauncher/tree/088bf5b435b1b63e842c11ff1c01648b5e38bced/docs) preserva planos e relatórios antigos.
