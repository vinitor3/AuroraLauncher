# Aurora Launcher 0.1.1-alpha.9

## Escopo integrado

Branch `codex/aur-release-alpha9`, worktree própria. A consulta dos chats e de `release:status` identificou duas entregas pendentes: retirada do Companion (`52ed803`) e grade Skins 0.2.5 (`1d4f8c8`, relatório `0a99319`). A segunda já contém a primeira. Todas as demais worktrees estavam limpas e integradas na alpha.8. O planejamento multiversão dos chats permanece planejamento, sem ampliar a matriz nesta release.

- Core 1.1.5 e Skins 0.2.5, grade de 12 slots, sem mod Companion; Assistente desktop preservado.
- Tradução opcional para português nas páginas de projetos Modrinth/CurseForge: botão circular, original recuperável, markup e fontes preservados; imagens e código não traduzidos. Serviço autenticado do Worker. Detalhes em [task-project-translation.md](task-project-translation.md).
- Catálogo como primeira opção de skins; escolher baixa e guarda o PNG, **Aplicar skin** salva a aparência sem exigir Minecraft aberto. Guarda por conta e proteção contra respostas atrasadas/atualizações concorrentes.
- Face da barra lateral usa o PNG aplicado localmente; camada opaca de skins legadas não cobre o rosto.
- Busca de jogadores com fallback oficial e distinção entre nome inexistente, limite e indisponibilidade. Detalhes em [task-player-skins.md](task-player-skins.md).
- Instalação de Minecraft/modpack concluída fecha detalhes/editor, limpa o filtro e mostra a instância selecionada com aviso. Falha de download não navega; falha posterior de listagem não é anunciada como falha de instalação.
- Controles próprios da janela; fechar mantém processo na bandeja, cujo menu permite abrir ou sair. Detalhes em [task-window-tray.md](task-window-tray.md).

## Validação

**CONCLUÍDO**: 100 testes Rust (85 biblioteca + 15 binário, 3 testes online ignorados), Cargo fmt/clippy e build desktop. Testes específicos cobrem seleção online, troca de conta, falha de download, instalação concluída/falha e atualização de listagem. O relatório final de empacotamento registra a contagem consolidada e a comparação dos 18 JARs.

**CONCLUÍDO**: teste em WebView2 nativo, com perfil de navegação isolado: botões de maximizar/restaurar e minimizar alteram o estado Windows; X deixa a janela invisível, mantendo IPC e processo ativos. O teste interage com os handlers reais via protocolo de depuração local, sem screenshot, login ou alterações de mundos/instâncias do usuário.

**CONCLUÍDO**: revisão independente encontrou e corrigiu a seleção antiga de skin em corrida com atualização da biblioteca e a perda de abertura externa dos links traduzidos.

**PRECISA DE TESTE**: clique físico no menu da bandeja, instalação em outro computador e seis pares da grade Skins ainda sem homologação gráfica. Esta release não declara todos os runtimes Minecraft homologados. Skins online exigem conexão; a aparência local já aplicada permanece utilizável se a sincronização falhar.

## Distribuição

Versão nova e imutável, gerada por `npm run release:package` depois dos commits e verificações. Instalador NSIS, SHA-256 e manifesto de origem/hash. Sem assinatura digital do instalador. Publicação do serviço e resultados dos testes online são registrados no relatório de entrega após execução.
