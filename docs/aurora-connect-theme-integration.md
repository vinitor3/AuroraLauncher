# Aurora Connect — ponto de integração com os temas

28/10/2026. Estado: **PARCIAL** — contrato registrado; merge e testes combinados **NÃO VERIFICADOS**. Complementa a [experiência social](aurora-connect-social-experience.md) e a [ADR-033](engineering/ADR/ADR-033-social-desktop-join.md), sem alterar gates funcionais.

## Bases observadas

Connect foi implementado no commit `a0d2e24e75ea48a52761f082ccf1678ad914d028`, branch `codex/aur-r4-connect`; o primeiro registro deste alinhamento é `ee304aa`. A primeira consulta somente leitura à worktree de temas `C:/Users/usuario/.codex/worktrees/launcher-themes/AuroraLauncher` encontrou HEAD `ffc37cc5f44ec74e3afa67cc9a81bf746a552d23` e mudanças em andamento. Após a conclusão da entrega, a branch `codex/aur-visual-themes` e o HEAD `3f30785e7344b6bc6841dfcee4c1c111cbf4999e` foram reconfirmados, com worktree limpa.

Foram lidos `docs/launcher-themes.md` e `docs/engineering/ADR/ADR-027-declarative-launcher-themes.md` daquela worktree. Na primeira consulta, `docs/theme-integration.md`, a nova seção de AGENTS e a API ThemeSurface ainda estavam pendentes. No commit concluído `3f30785`, foram lidos integralmente `docs/theme-integration.md`, `AGENTS.md`, `ThemeSurface.tsx` e `surface.css`; os contratos de navegação TypeScript/Rust foram conferidos. Esta atualização substitui a pendência de leitura, sem declarar merge ou testes combinados.

O ancestral comum entre `ee304aa` e `3f30785` é `dc2ec5ac36a88f54b1130bac4789f8ecd56641d2`. Os dois commits exclusivos de temas são `ffc37cc` e `3f30785`; o segundo depende da base do primeiro. A matriz ativa nos dois checkouts conserva SHA-256 `96c4ec789902f8aa87931ecd72aca531fec53d4ac9c6055fcb9837ef77e6918a`.

Não copiar arquivos inteiros entre worktrees, nem aplicar o `App.tsx` de uma sobre a outra. Integrar a entrega revisada de temas cumulativamente com Connect, barreiras de conta, CAS/Doctor/Recovery, Assistente e nove pares Core/Skins. Não assumir que o AGENTS da branch de temas contém os checkpoints Connect posteriores. A integração final das branches continua separada deste alinhamento; nenhuma branch de terceiros foi alterada.

## API visual conferida em 3f30785

`ThemeSurface` recebe somente `surfaceId`, `as` (`section`, `article` ou `div`), `labelledBy`, `className` e `children`. Não cria provider nem recebe handlers. `surfaceId` é constante da feature, exposta em `data-theme-surface`; não acrescenta um item de navegação, campo JSON ou associação por painel. O ID acessível do título pode usar `useId`, sem confundi-lo com essa identidade estável.

O CSS oferece fallback oficial de superfície opaca, texto, borda, fonte, escala e espaçamento. Classes opcionais: `theme-surface-heading`, `theme-surface-description` e `theme-surface-action`. O painel mantém seu layout, foco e estados; dados essenciais e erros usam o texto principal. O contrato proíbe ativar/importar automaticamente temas recebidos de servidores, modpacks ou sessões Connect. Confirmações sensíveis e recuperação ficam fora da apresentação personalizada.

Na integração, superfícies comuns podem adotar esses tokens com fallback, enquanto revisão de autorização e controles de segurança mantêm aparência oficial independente. Nenhuma importação desse componente foi adicionada antes da integração de sua dependência. As provas informadas pelo responsável de temas pertencem à sua entrega; não foram reexecutadas nem atribuídas ao Connect neste registro.

## Fronteira obrigatória

Temas são documentos locais JSONC fechados de apresentação. Não executam código, não chamam IA e não definem URLs, credenciais, convites, autorização, sessão, transporte ou callbacks. Cores, tipografia, espaçamento, ícones, mídias e cursor usam somente os campos suportados e validados nos dois lados do contrato.

| Ponto existente do Connect | Integração visual futura | Responsabilidade preservada |
| --- | --- | --- |
| `App.tsx`, seções `connect` e `social` | Adotar os IDs estáveis no contrato de navegação de temas | Disponibilidade, seleção e callbacks continuam no App; tema não habilita capability experimental. |
| `SocialView.tsx` e cards | Usar `ThemeSurface` quando sua API estiver integrada e variáveis `--theme-*` com fallback oficial | Busca, amizade, bloqueio, anúncio e Entrar mantêm validação e callbacks confiáveis. |
| `SocialConnectionPanel.tsx` | Superfícies de informação podem receber apresentação suportada | Estado vem do snapshot nativo; autorização, consentimento, erros e controles de segurança não podem ser ocultados, sobrepostos ou substituídos pelo tema. |
| `ConnectView.tsx` | Preservar observação e controles do mesmo manager | Nenhuma confirmação automática ou exportação manual é introduzida em uma tentativa social. |
| `connect-controller.ts` e comandos Tauri | Sem configuração funcional no tema | Mesmo owner/slot/epoch/geração, token em RAM, validação, cancelamento e cleanup. |
| Confirmação/restauração de temas | Manter a camada oficial independente da área tematizada | Recuperação e prazo de confirmação não dependem do tema aplicado. |

Os IDs `connect` e `social` já existem em `LauncherSection` desta base. O contrato de temas observado ainda listava quatro itens (`instances`, `discover`, `appearance`, `java`). A integração deve atualizar em conjunto enumeração/frontend, validação Rust, defaults/migração de documentos antigos, editor, comentários e testes. Não tornar temas antigos inválidos só porque novas seções foram acrescentadas; também não aceitar IDs arbitrários nem permitir que o tema remova ações de segurança. Se uma seção estiver indisponível por gate funcional, o tema não pode torná-la disponível.

## Verificação no merge futuro

- Preservar Entrar público/amigos, consentimento anfitrião, stop/logout e respostas tardias com tema oficial e personalizado; aparência não cria nem recria sessões.
- Conferir migração de temas antigos, fallback, contraste, foco/teclado, cards/lista e mensagem de erro sem perda de informação.
- Confirmar que mídias/cursor/decorações não interceptam cliques em Entrar/Desconectar ou cobrem a revisão e a recuperação.
- Reexecutar testes de navegação/tema e focais sociais; observar o executável real antes de declarar a integração gráfica validada.

Este registro não altera código, dados pessoais, a worktree de temas, recursos Cloudflare ou resultados do Social89. A verificação desta alteração documental é `git diff --check`; não exige rebuild. Não autoriza publicação, deploy, instalador ou mudança dos gates de Minecraft, UDP, voz e redes.
