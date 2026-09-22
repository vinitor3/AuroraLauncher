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

**CONCLUÍDO**: 107 testes Rust (85 biblioteca + 22 binário, 4 testes online ignorados na execução padrão), Cargo fmt/clippy, build desktop, 90 testes frontend e 82 testes Worker. A consulta pública real foi executada separadamente e aprovada para Notch, jeb_ e Dinnerbone. Testes específicos cobrem seleção online, troca de conta, falha de download, instalação concluída/falha e atualização de listagem. O relatório final de empacotamento registra a contagem consolidada e a comparação dos 18 JARs.

**CONCLUÍDO**: teste em WebView2 nativo, com perfil de navegação isolado: botões de maximizar/restaurar e minimizar alteram o estado Windows; X deixa a janela invisível, mantendo IPC e processo ativos. O teste interage com os handlers reais via protocolo de depuração local, sem screenshot, login ou alterações de mundos/instâncias do usuário.

**CONCLUÍDO**: revisão independente encontrou e corrigiu a seleção antiga de skin em corrida com atualização da biblioteca e a perda de abertura externa dos links traduzidos.

**CONCLUÍDO**: tradução autenticada real pelo Gemini e teste do componente real em WebView2, preservando estrutura, negrito, código, link externo e restauração do original. Busca real de Notch por nome/UUID com PNG e SHA-256 conferidos; download, gravação e recarga nativos do PNG em identidade local exclusiva de QA.

O smoke ampliado encontrou falhas por nome em outros jogadores: timeout no índice e HTTP 403 das APIs oficiais quando acessadas pelo Cloudflare. A integração acrescenta resolução por nome atual → UUID no índice e consulta oficial nativa no desktop, usando a conexão do usuário quando o Worker retorna indisponibilidade/404. Não fabrica skins para contas desconhecidas e não contorna limites HTTP 429.

**PRECISA DE TESTE**: clique físico no menu da bandeja, instalação em outro computador e seis pares da grade Skins ainda sem homologação gráfica. Esta release não declara todos os runtimes Minecraft homologados. Skins online exigem conexão; a aparência local já aplicada permanece utilizável se a sincronização falhar.

**CONCLUÍDO**: Worker publicado na versão `14c1ce81-9bc0-45a8-abf3-30c25bf7a516`; buscas reais jeb_/Dinnerbone retornam HTTP 200. Smoke real no desktop forçando Worker 503: ambos carregam pela origem `mojang-local` com hash válido, enquanto um nome inexistente retorna “Jogador não encontrado”. Não houve mudança de aparência da conta do usuário.

## Distribuição

**CONCLUÍDO**: instalador gerado por `npm run release:package` em 22/09/2026, a partir do commit `a0b417c832b97c5112f8e133cf4b91b5e2ff5f4d`. Preflight inicial/final aprovados com 19 worktrees; Core 9/9 + Skins 9/9 recompilados e comparados. Nenhum artefato histórico foi sobrescrito.

- Arquivo: `Aurora Smart Launcher_0.1.1-alpha.9_x64-setup.exe`.
- Tamanho: 6.813.927 bytes.
- SHA-256: `17a12443ab08207c8c5e8b79ff46e7c4aac7dc752eec99d72fde285b895dfeed`.
- Manifesto e checksum ao lado do instalador; sem assinatura Authenticode.
- Teste no executável final de release repetiu com sucesso maximizar/restaurar/minimizar e fechar para bandeja, com IPC vivo. O instalador não foi executado sobre a instalação do usuário. O campo runtime do manifesto permanece conservador, sem alegar homologação Minecraft completa.

Logs locais em `.aurora/release-runs/20260922-105612-02044836/build.log` e smokes em `.aurora` (não versionados). Os processos e o PNG da identidade exclusiva de QA foram encerrados/removidos; mundos e aparência da conta não foram alterados.
