# AUR-1166 — Recuperação manual de instâncias

Estado: **IMPLEMENTADO MAS NÃO HOMOLOGADO**. A validação desta entrega usa somente instâncias temporárias e arquivos sintéticos. Execução nativa completa com jogo real permanece **PRECISA DE TESTE**.

## Base preservada

Repositório ativo: `C:/Users/usuario/Desktop/AuroraLauncher`. Branch de trabalho: `codex/aur-1166-instance-recovery`, worktree exclusiva. Base conferida antes da implementação: `dc2ec5ac36a88f54b1130bac4789f8ecd56641d2`, launcher `0.1.1-alpha.13`, Core `1.1.5` e Skins `0.3.1` em `scripts/active-mod-matrix.json`. A base inclui Doctor, orientação Gemini, histórico, ações confirmadas, sessões independentes e o failover Gemini posterior à alpha.13. Nenhum código foi copiado de `TESTE-main/aurora-skins-runtime`.

A tarefa não altera versões, pares de Minecraft/loaders/Java, JARs distribuídos ou implantação do Worker. Não gera release nem instalador. O Assistente permanece desktop e Companion não retorna.

## Implementação

Fluxo integrado de pontos manuais: captura consistente, listagem, detalhes, exclusão confirmada, prévia de restauração, confirmação, proteção do estado anterior e desfazer. O backend usa CAS SHA-256 com blobs independentes e imutáveis, manifesto versionado, publicação transacional, staging e journal persistente. A interface explica exclusões, mundos e módulos administrados.

Contrato preciso: [Recuperação de instâncias](instance-recovery.md). Decisão: [ADR-027](engineering/ADR/ADR-027-manual-instance-recovery.md), que aplica a [ADR-006](engineering/ADR/ADR-006-local-cas.md) apenas a este domínio; snapshots de updates e backups do Assistente continuam com seus contratos existentes.

Arquivos centrais: `apps/desktop/src-tauri/src/engine/recovery.rs`, integração do bloqueio em `engine/content_updates.rs`, comandos em `commands.rs` e registros em `main.rs`; interface em `apps/desktop/src/features/recovery/`, gerenciamento em `App.tsx` e atalho no `DoctorView.tsx`. SQLite (`rusqlite` com biblioteca empacotada) e verificação de espaço (`fs2`) são dependências locais do núcleo, sem serviço externo.

## Validação

Testes automatizados com filesystem real em diretórios temporários comprovam somente os cenários exercitados; testes DOM comprovam comportamento dos componentes com transporte simulado. Nenhum dos dois equivale a homologação nativa de uma instância pessoal ou runtime Minecraft.

| Verificação | Resultado executado |
| --- | --- |
| `cargo test --test instance_recovery_qa` | **CONCLUÍDO**: 17 testes independentes aprovados, nenhum ignorado; filesystem real em fixtures temporárias, incluindo junction real do Windows e restauração/desfazer dos bytes e estados ativo/desativado de JARs sintéticos. |
| Testes unitários de Recuperação e journal | **CONCLUÍDO**: 14 testes aprovados, incluindo falhas injetadas, publicação inicial de identidade, perfil e retomada por journal. |
| `cargo test --test recovery_listing` | **CONCLUÍDO**: dois testes aprovados para pontos corrompidos/ausentes sem ocultar pontos íntegros e exclusão de registros indisponíveis. |
| `RecoveryView.smoke.mjs` | **CONCLUÍDO**: 18 grupos DOM aprovados com comandos nativos simulados, incluindo confirmação, desfazer, erro, listagem indisponível e sucesso já confirmado com falha na atualização da lista. |
| `npm --prefix apps/desktop run build` | **CONCLUÍDO**: TypeScript/Vite aprovados; aviso de bundle acima de 500 KiB, sem erro de build. |
| `DoctorView.smoke.mjs` | **CONCLUÍDO**: 22 grupos aprovados, incluindo acesso à Recuperação. |
| `AiTools.smoke.mjs` | **CONCLUÍDO**: 18 grupos aprovados. |
| `node --test src/lib/doctorAdvice.test.mjs` | **CONCLUÍDO**: sete testes aprovados. |
| `cargo fmt --all -- --check` | **CONCLUÍDO**. |
| `cargo clippy --all-targets -- -D warnings` | **CONCLUÍDO**. |
| `cargo test` completo | **CONCLUÍDO**: 225 aprovados, cinco testes opt-in preexistentes ignorados, nenhuma falha (121 lib, 40 bin e 64 integração). Registro local: `.aurora/recovery-cargo-test.log`. |
| `cargo build` debug | **CONCLUÍDO**; compilação do executável nativo, sem gerar instalador, release ou publicar artefato. |
| UI nativa com restauração completa de uma instância real | **PRECISA DE TESTE**; as evidências automatizadas acima não comprovam essa homologação. |

Os testes independentes de Recuperação estão em `apps/desktop/src-tauri/tests/instance_recovery_qa.rs`; testes de falha e journal em `engine/recovery.rs` e `engine/recovery_journal_tests.rs`. A correspondência com os critérios solicitados é:

| Critério | Teste/evidência automatizada |
| --- | --- |
| Captura/restauração por bytes e inventário; adicionar, modificar e remover | `protected_bytes_round_trip_and_undo_without_touching_excluded_data` compara mapas de caminho→bytes e exige adição, substituição e remoção na prévia. |
| JARs e estado ativo/desativado | `jar_bytes_and_active_disabled_off_states_round_trip_through_restore_and_undo` captura JARs sintéticos válidos `.jar`, `.jar.disabled` e `.jar.off`, altera nomes/bytes, adiciona e remove arquivos e exige inventário e bytes exatos após restauração e desfazer. Nenhum JAR é executado. |
| Mundos e caminhos excluídos | O mesmo teste conserva bytes de `saves`, screenshots, logs, resourcepacks e natives; `credential_like_configs_and_unprotected_profile_links_remain_current` conserva dados sensíveis sintéticos e campos não permitidos. |
| Desfazer | O teste de roundtrip exige retorno dos bytes e inventário anteriores; smoke UI exige outra revisão e confirmação. |
| Ausência/corrupção | `missing_and_corrupt_blobs_never_reach_commit`; testes de `recovery_listing` conservam acesso aos pontos íntegros. |
| Interrupção e recuperação | `interrupted_restore_reopens_from_disk_and_blocks_ordinary_mutations`, `interruption_between_original_rename_and_publish_recovers` e `completed_file_renames_still_rollback_after_restart_before_index_commit` descartam o contexto da operação e recuperam a partir do disco. |
| Falta de espaço/falha de escrita | `no_space_and_staging_write_failure_leave_protected_bytes_unchanged` injeta falhas antes da aplicação; não foi enchido o disco físico do usuário. |
| Jogo e concorrência | `recovery_and_existing_mutations_share_the_same_exclusion_lock`; `recovery_gate_probes_again_under_the_shared_game_lock` verifica o gate de processos antes e depois do lock e sua liberação em erro. `process_probe_refuses_unlocated_minecraft_but_allows_unrelated_java` recusa processos Minecraft reconhecidos sem `--gameDir` e permite Java não relacionado. Probe usa respostas controladas, sem executar Minecraft. |
| Captura instável/prévia obsoleta | `capture_aborts_if_external_writer_changes_inventory_before_publish`, `modifying_a_saved_point_source_after_preview_invalidates_confirmation`, `preview_binds_manifest_and_unknown_profile_fields` e `rewriting_a_manifest_after_preview_requires_another_review_even_with_valid_hash`. |
| Excluir sem afetar outro ponto/CAS sem hardlinks | `duplicate_points_share_content_and_deleting_one_keeps_the_other_usable` e `stored_objects_are_independent_copies_and_identical_points_share_bytes`. |
| Manifestos/caminhos maliciosos | `checksummed_malicious_manifests_cannot_expand_scope_or_escape_paths` recalcula o hash no índice para alcançar a validação semântica; `a_real_windows_junction_in_scope_never_reads_or_changes_its_target` e `leaf_hardlinks_cannot_mutate_or_reuse_external_backup_content` verificam links reais. |
| Identidade, renomeação, cópia e recriação | `instance_rename_retains_points_but_reusing_its_old_name_does_not`, `copying_an_identity_marker_never_authorizes_another_directory`, `correcting_a_copied_markers_physical_identity_does_not_forge_the_registry` e `another_instances_guard_cannot_authorize_capture`. |
| Interrupção durante a primeira identidade | `interrupted_first_identity_publication_does_not_strand_an_untouched_instance` conserva a instância utilizável e permite uma captura posterior válida. |
| Módulos administrados/ambiente | `renamed_managed_artifacts_are_excluded_and_never_overwritten_by_an_older_point` cobre conteúdo Aurora renomeado e Companion; `changed_runtime_cannot_replay_mods_into_a_different_loader` bloqueia ambiente diferente. |
| Journal malicioso e alterações externas na recuperação | `poisoned_in_scope_journal_cannot_remove_an_unapproved_file`, `profile_journal_cannot_restore_modified_internal_links` e `rollback_preserves_external_edits_and_stays_blocked_instead_of_overwriting_them`. |
| Conflito entre arquivo e diretório | `a_file_directory_type_conflict_is_rejected_before_a_pending_journal_can_strand_the_instance` preserva os bytes e não deixa journal pendente em ambas as direções. |

## Limitações e próximo incremento

Não inclui mundos, screenshots, logs, caches, Java, bibliotecas/instalação compartilhada, contas, credenciais ou sessões. Pontos não substituem Core/Skins administrados nem reinstalam Companion. A exclusão conserva blobs sem referências e não representa liberação imediata de disco.

A allowlist do perfil é somente `aurora-modpack.json.name`; versão/loader e vínculos permanecem atuais e são revalidados. Configurações suspeitas, não textuais ou maiores que 4 MiB ficam explicitamente excluídas. O filtro de texto não é um detector universal de segredos. Conversão de arquivo em diretório ou vice-versa exige resolver o conflito manualmente antes da restauração. Queda real de energia, disco fisicamente cheio e homologação completa na UI nativa/Minecraft permanecem **NÃO VERIFICADO**; falhas injetadas e testes DOM não equivalem a essas situações reais.

Não foram executados rebuilds de Core/Skins, deploy/check do Worker nem lint do backend legado, pois esses componentes não receberam mudanças. A versão e a matriz ativa foram conferidas e preservadas. Nenhuma instância pessoal foi usada nos testes destrutivos.

Pontos automáticos antes de atualizações são o próximo incremento e permanecem **NÃO IMPLEMENTADO** nesta tarefa. Também permanecem fora do escopo restauração por IA, novas correções automáticas, diagnóstico em cópia temporária, mundos e backup em nuvem.
