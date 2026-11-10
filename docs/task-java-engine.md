# Tarefa — Java e engine

Data: 28/10/2026. Branch `codex/aur-java-engine`. Worktree exclusiva `C:/Users/usuario/.codex/worktrees/aur-java-engine/AuroraLauncher`.

## Base e confronto

Raiz Git comum: `C:/Users/usuario/Desktop/AuroraLauncher/.git`. A interface `TESTE-main` contém o projeto histórico e não foi usada como base. Conferidos `AGENTS.md`, HEAD e matriz antes de criar a worktree. Início em `dc2ec5a`; conferência adicional encontrou a entrega concluída alpha.14 `6a107b8`, descendente direta, e a branch avançou por fast-forward antes de implementar. Preservados CAS/recuperação/Doctor. O ramo Connect `a0d2e24` também descende dessa base, com recursos próprios; não foi mesclado. A worktree de temas parte de `ffc37cc` e tem alterações em andamento, preservadas.

Matriz: Core **1.1.5**, Skins **0.3.1**, nove alvos por módulo; Java 8/17/21 conforme Minecraft/loader. Não houve alteração de módulos ou artefatos. Nenhum instalador gerado nem release publicada. Contrato visual conferido na entrega concluída `3f30785` (`docs/theme-integration.md`, `ThemeSurface.tsx`, `surface.css`, `AGENTS.md` e `launcher-themes.md`); ela não foi mesclada sobre a base alpha.14. O painel consome diretamente os mesmos tokens/fallbacks, e o patch de montagem aplica sem conflito também nesse commit de temas.

O gerenciador Java e os requisitos Fabric já existiam. DOCX `Aurora_Status_e_Reorganizacao_2026-08-29.docx`, parágrafos 297–300/646–650/1070–1074, confirma JVM-01/OPEN-07: fórmula implementada/testada, mas launch fixa 4096 MB; inventariar RAM/mods e aplicar flags por Java. A documentação não fixa flags GC ou hooks. Controles RAM/JVM vêm do pedido atual; não foram deduzidos como autorização para shell.

## Critérios de aceite

- [x] RAM manual/automática e inicial opcional são persistidas e revalidadas pelo caminho nativo de launch, com heap máximo único comprovado em teste de composição. Execução no Minecraft ainda **PRECISA DE TESTE**.
- [x] Fórmula existente recebe inventário físico Windows/JARs com limites; pouca RAM/probe indisponível não produz `-Xmx0M`.
- [x] Política GC por Java é explícita e conflitos são recusados/substituídos de modo determinístico.
- [x] Argumentos permitidos chegam como `argv`; shell, agents, argfiles, heap duplicado, classpath/identidade/IPC não podem ser personalizados.
- [x] Ausência de configuração preserva padrões; arquivo inválido não é ignorado; salvar é protegido e falha de publicação Windows conserva o arquivo anterior em teste real de handle bloqueado.
- [x] Doctor usa memória efetiva; configuração inválida não desativa o diagnóstico. Locks/recuperação alpha.14 e Assistente/Core/Skins são preservados no código e nas regressões automatizadas.
- [x] Painel com tokens oficiais cobre leitura/salvar/cancelar/padrões, erros privados, locks e resposta atrasada em smoke DOM.
- [x] Patch de montagem em `App.tsx` é pequeno, passa `git apply --check` na base alpha.14 e no contrato de temas `3f30785`, e o candidato completo passa TypeScript sem editar o arquivo real.

## Implementação e riscos

Contrato completo em [java-engine.md](java-engine.md), decisão em [ADR-034](engineering/ADR/ADR-034-instance-java-engine-settings.md). Estado geral: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. A montagem do painel no app integrado permanece **PRECISA DE TESTE** e depende da aplicação do patch pelo dono de `App.tsx`.

**RISCO:** RAM total não representa RAM livre; jogos simultâneos competem pela mesma memória. A política G1 não é benchmark. Flags avançadas podem ser recusadas por Java/arquitetura. RAM automática não está implementada fora do Windows. A validação de caminhos não pretende conter um processo local hostil alterando diretórios simultaneamente. Configurações de launch não pertencem aos pontos de recuperação existentes.

## Validação

Testes e gates finais abaixo; sem homologação Minecraft ou equivalência entre smoke sintético e UI nativa.

- `npm --prefix apps/desktop run build`: **CONCLUÍDO**, somente aviso de bundle grande preexistente. O componente isolado está incluído na checagem TypeScript; ainda aguarda montagem no app.
- `node apps/desktop/src/features/java-engine/LaunchSettingsPanel.smoke.mjs`: **CONCLUÍDO**. Leitura, salvar/cancelar/padrões, RAM inválida, troca para automático, erro privado, locks, StrictMode, unmount e respostas atrasadas.
- `node --test apps/desktop/src/lib/assistantPreservation.test.mjs`: **CONCLUÍDO**, 10 testes; `DoctorView.smoke.mjs`: **CONCLUÍDO**. São adaptadores sintéticos, sem prova Gemini/voz/microfone.
- Composição/persistência: testes incluem RAM física Windows, Unicode malformado, limite 63/64 com G1, schema desconhecido, pastas/links, guard errado, enumeração limitada e substituição bloqueada que preserva bytes anteriores.
- A primeira suíte nativa com paralelismo padrão teve falha transitória no teste existente `engine::ipc::tests::rejects_invalid_nonce_or_protocol` (`WouldBlock`/Windows 10053). A repetição isolada passou; o conjunto com `--test-threads=4` passou. Não foi alterado código de IPC para mascarar o resultado.
- Verificação visual: **NÃO VERIFICADO**. O navegador integrado não estava disponível e o controle nativo foi interrompido pelo CUA por não identificar a URL do Chrome com confiança. Harness sintético ficou somente em `.aurora/java-engine-visual`; servidor próprio 1534 encerrado, sem tocar no Vite 1420 ou no launcher de teste de temas. Nenhuma captura ou ação visual foi concluída.

Gate nativo final: **CONCLUÍDO**. `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test -- --test-threads=4` passaram: **289 aprovados, 0 falhas, 5 ignorados** (testes online/externos opcionais). Logs locais ignorados pelo Git em `.aurora/java-engine-validation/`. A revisão independente resultou em correções de UTF-8, limite de argumentos com GC, máximo manual inválido ao trocar para automático e preservação do Doctor quando as preferências estão quebradas. A leitura de requisitos Java continua depois da migração/instalação gerenciada, para não deixar um Companion antigo bloquear sua própria retirada.

Arquivos principais: `engine/launch_settings.rs`/testes (schema, resolução e persistência), `engine/launch.rs` (composição/spawn), `commands.rs`/`main.rs`/`engine/mod.rs` (integração nativa), `features/java-engine/*` (painel/types/CSS/smoke), patch de `App.tsx` e documentação de domínio/ADR/matriz. `App.tsx`, temas, `DesktopWindow`, Core, Skins e Worker não foram editados nesta branch. A última verificação do patch também passou contra `3f30785`, sem aplicar mudanças na worktree de temas.

## Roteiro restante de homologação

Após integrar o patch com temas/Connect, iniciar a build desktop de desenvolvimento: salvar RAM manual e automática em duas instâncias, reabrir e comparar Doctor; iniciar/encerrar uma instância real Java 8/17/21, comprovar heap observado e IPC/Core/Skins por linha da matriz; testar configuração inválida e JVM que recusa flag; verificar temas antigos/atuais e fallback visual; validar Assistente sem Minecraft (painel/atalho/chat/voz separadamente). Registrar versões, hashes e resultados reais antes de promover o estado.
