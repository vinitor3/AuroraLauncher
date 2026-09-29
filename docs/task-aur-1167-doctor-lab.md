# AUR-1167 — Doctor, recuperação e cópias de diagnóstico

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. Validação de integração e empacotamento em andamento.

## Base e preservação

- Repositório ativo conferido em 29/09/2026: `dc2ec5ac36a88f54b1130bac4789f8ecd56641d2`, branch `codex/aur-1165-multiplayer`, launcher `0.1.1-alpha.13`.
- Matriz preservada: Core `1.1.5`, Skins `0.3.1`, nove alvos cada. Companion permanece retirado.
- Doctor local `8551c54`, orientação Gemini `0ed345f` e ações/histórico `db74fb5` já eram ancestrais da base. Failover Gemini `cab7bb1` e validação `dc2ec5a` preservados.
- Recuperação manual concluída em `codex/aur-1166-instance-recovery`, commit `be3a1c4d9bb883c872708c2ace242e5b6c08e067`, integrada integralmente por fast-forward. Nenhuma alteração pendente nessa worktree.
- `release:status` inicial: única entrega pendente era recuperação; demais 26 worktrees integradas e sem alterações de produto.
- Trabalho exclusivo em `codex/aur-1167-doctor-lab`. Ferramenta gerenciada de worktree indisponível para o cwd deste chat (`Not a git repository`); criada com Git a partir do HEAD ativo conferido. Nenhuma worktree alheia editada.

## Plano e responsabilidade

1. Compartilhar observação verificada e identidade com a recuperação existente.
2. Registrar tentativas e diferenças observadas no Doctor, com limites de histórico explícitos.
3. Criar cópias de teste independentes e aplicar apenas mudanças selecionadas, usando prévia, proteção, journal e desfazer da recuperação.
4. Conectar gerenciamento e Doctor; revisar concorrência, caminhos, corrupção, interrupções e conflitos em instâncias descartáveis.
5. Validar o conjunto, sincronizar uma versão inédita, fazer commit e executar um único `release:package` oficial.

Propriedade durante implementação: agente de backend (recuperação e módulo de cópias), agente de histórico (associação Doctor), agente de interface (`App.tsx` e telas); coordenador (`commands.rs`, registro dos comandos, integração, release). Revisão independente após entrega dos módulos. Nenhum teste destrutivo usa instâncias pessoais.

## Validação

| Verificação | Resultado |
| --- | --- |
| Build TypeScript/Vite da árvore integrada alpha.14 | **CONCLUÍDO**, aviso preexistente de chunk grande; `.aurora/desktop-build.log`. |
| Bibliotecas frontend (`node --test`) | **CONCLUÍDO**, 39/39; transporte, tradução, preservação do Assistente e orientação Doctor; `.aurora/frontend-libraries.log`. |
| Smokes DOM Doctor, Recovery, AI Tools e cópias de teste | **CONCLUÍDO**, quatro scripts aprovados. Incluem seleção explícita de mundo, confirmação, retorno para desfazer, prévia invalidada, resposta atrasada, bloqueio em execução e caminho de launch normal. Logs `.aurora/smoke-*.log`. |
| Worker: `check` sem deploy e testes | **CONCLUÍDO**, dry-run aprovado, 155/155 testes; `.aurora/edge-check.log` e `.aurora/edge-tests.log`. |
| Backend legado: lint TypeScript | **CONCLUÍDO**; `.aurora/functions-lint.log`. |
| Cargo check de todos os targets alpha.14 | **CONCLUÍDO**; `.aurora/integration-check.log`. |
| Testes focados DoctorRuns e Doctor/recuperação | **CONCLUÍDO**, 11 e 8 respectivamente no agente proprietário; serão incluídos também na suíte final. |
| Rustfmt e Clippy de todos os targets (`-D warnings`) | **CONCLUÍDO**, sem erros nem avisos; `.aurora/clippy.log`. |
| Suíte Cargo final completa | **CONCLUÍDO**, 258 aprovados, cinco testes opt-in preexistentes ignorados, nenhuma falha. 150 lib, 42 bin, 66 integração. Inclui 21 testes novos de cópias, oito de histórico verificado e dois cenários independentes de integração. `.aurora/cargo-test-final.log`. |

A primeira execução completa terminou com uma falha no teste IPC preexistente `core_events_survive_and_retired_companion_messages_are_ignored`: handshake interrompido e Windows 10053 (`ConnectionAborted`). Nenhuma verificação foi desativada. Depois das correções finais de validação de perfis e de dois avisos de estilo apontados pelo Clippy, a suíte completa foi repetida e passou, inclusive esse teste. O IPC não recebeu alteração; a falha inicial permanece registrada em `.aurora/cargo-test.log`.

Revisão independente identificou e corrigiu: visibilidade de cópias interrompidas antes do manifesto final; fixação do SHA-256 dos bytes construídos para impedir transferência de campos não selecionados por uma edição externa do staging; preservação de logs legados sem promovê-los a evidência verificada; comparação de cada tentativa de teste com a base de criação; retomada da exclusão após remoção do marcador; validação de credenciais e caminhos de perfis de runtime. A revisão é adicional aos testes, não homologação.

Build e testes automatizados não equivalem a homologação na UI nativa, Minecraft, autenticação online ou voz. Queda física de energia e disco fisicamente cheio permanecem **NÃO VERIFICADO**; os cenários equivalentes usam interrupções determinísticas e falhas injetadas. Pontos automáticos gerais antes de atualizações continuam **NÃO IMPLEMENTADO**. A exclusão da cópia conserva o CAS e histórico da recuperação segundo a política anterior: libera sua pasta de jogo, sem coleta insegura de conteúdo compartilhado.

Versão inédita selecionada: `0.1.1-alpha.14`; nenhuma tag, commit de release ou artefato com esse nome existia na consulta local. Sincronizados package root/desktop, lockfile npm, Cargo/lock e configuração Tauri. A matriz Core/Skins permanece inalterada.
