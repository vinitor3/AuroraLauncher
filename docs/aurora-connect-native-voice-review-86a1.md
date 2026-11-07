# Connect86A1 — preparação e revisão nativas privadas

Base local `142c869e202e63b409f8cc7332cfa73bcfa77f2c`, 07/11/2026. Estado: **PROTÓTIPO**, com focais locais aprovados; o aceite86A/B permanece **PRECISA DE TESTE**. Continuação estritamente privada de [86A0](aurora-connect-native-voice-binding-86a0.md), sob Windows + `connect-lab` + `connect-voice-lab`, default-off. Este checkpoint não satisfaz o aceite86A/B: não abre Host, exporta convite, confirma transporte nem implementa retiradaUDP. Não acrescenta UI/IPC, Java, configuração, relay ou áudio.

## Bloqueio e implementação

86A0 fornecia o binding fraco e tokens consumíveis do QueryJob84, mas não possuía caller assíncrono registrado na barreira do ConnectManager.86A1 acrescenta entradas Rust privadas `prepare_native_voice_group` e `review_native_voice_group`, usando o mesmo Attempt e `group_source::SourceWork`. Não existe novo slot de origem nem proprietário de processo.

O preparo fixa roster2–8/distinto, limitado também por `Limits.max_peers`, modo loopback literal e endereço inicial antes da observação. Origem, destinosTCP43001/UDP43002ou43003 e perfil derivam exclusivamente do binding. Não recebe PID, path, alvoTCP/UDP ou perfil como substituto da origem. Registra o trabalho no Attempt, reserva o mesmo job84 e somente depois despacha. O resultado público Rust é apenas um receiver; perdê-lo não perde o worker. Falha/stop/expiração invalidam a operação, mas cleanup espera a cauda bloqueante original.

A janela fixa2s começa antes do dispatch. O ator observa cancelamento, identidade/geração/fase, limite nativo e prazos enquanto aguarda. Vencimento envia recusa sem esperar a syscall, mantém job/worker registrado e descarta resultado favorável tardio. O ramo de resultado e os gates de publicação também conferem o deadline; ordem de `select` não pode aceitar observação tardia. Não há retry, cache favorável ou renovação deTTL.

A revisão consulta novamente a sequência85 completa fora do mutex Connect, emite cada `SessionInvitation`schema3 uma vez e retém os objetos originais. Publica somente o roster inteiro após revalidar origem, prazos civis/monotônicos e cada ingresso. Preparo≤600s, limitado pelo deadline nativo original; revisão≤60s, limitada pelo preparo, origem e ingressos. Geração/operação/proprietário divergentes recusam o caminho. Fases privadas impedem que confirmação/exportação manual/descoberta anterior consuma a revisão nativa.

O digest novo usa exclusivamente o domínio aprovado86, proprietário, nonce/perfil originais e dados em ordem fixa: `operation_id`, `generation`, `review_id`, `host_public_id`, `mode`, `direct_address`, `game_target`, `udp_target`, `announcement_policy`, `services`, `participants`, `expires_at_ms`. Nomes são snake_case no JSON compacto privado; serviços incluem protocolo, revisão, capability, limite e política/orçamento de retorno. Participantes preservam ordem, IDs, destinatários, digest do envelope original e os dois prazos. O digest não participa de sua preimagem. O nonce não aparece no resultado Rust, snapshot ou log. A revisão apresenta separadamente o rótulo da cópia original e versões compiladas, sem path físico, conteúdo de configuração ou chave privada.

`RuntimeUnavailable`, variante nova do caller91, foi acrescentada explicitamente à tabela86 como falha de integração/observador. Ela não é diagnósticoTCP nem motivo de fallback. Falhas85 mantêm classificação tipada, inclusive Busy aninhado e os subtipos de configuração; nenhuma usa texto de erro.

## Evidência e limites

Root executou, serializado e sem fixture concorrente nas portas dedicadas:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --lib native_voice -- --test-threads=1
```

Resultado: **13 PASS / 0 falhas / 0 ignorados**,238,52s de testes e1,30s de compilação incremental. Inclui sete pais86A0 já existentes, cinco pais novos de fixtures reais86A1 (cada um percorre SVC ePlasmo separadamente) e um novo pai de preimagem. São **seis pais novos**, não13. Log privado `.aurora/connect-validation/resume90-91-native_voice.log`. Compilação inicial do conjunto90/91/86A1 aconteceu antes deste filtro; seu custo não pertence ao1,30s incremental. Primeira conferência conjunta recusou o match não exaustivo após91 introduzir `RuntimeUnavailable`; a correção explícita e tipada precedeu esta rodada. Checks finais pertencem ao registro conjunto do root.

Fontes adicionadas: `connect/native_voice.rs`, `connect/native_voice_review.rs`, `connect_dedicated/tests/native_voice_review.rs`; hooks estreitos em `live.rs`, tracking em `group_source.rs`, inicializadores e projeções imutáveis privadas do produtor. Não há novos arquivos no Core/Skins nem alteração dos nove pares.

Os cinco pais reais exercitaram ambos os perfis com o filho próprio85/STATUS/UDP e tabelasOS, endereço de transporte mantido ocupado por socket do teste, revisão/envelopes originais, rejeição de configuração alterada comTCP saudável, job retido além da janela, perda de waiter e stop pendente. O caso de dispatch usa runtime com uma thread bloqueante ocupada: reserva84 já impede nova query antes do primeiro await; depois que o dispatch real foi observado, stop do dedicado não publica completion/restauração enquanto o mesmo job permanece enfileirado. A liberação do worker original permite recolher cleanup eEULA. Isto não equivale a Host/Guests, MinecraftJava, autenticação SVC/Plasmo ou áudio.

O teste de preimagem confere bytes compactos exatos e domínio próprio, proprietário/nonce/perfil e campos da revisão; a comparação diferencial usa o mesmo encoder em ambos os lados para não confundir ordem de chaves com mutação. Revisão independente90 e root não encontrou bloqueador neste checkpoint. O teste não converte digest ou amostra em autoridade nativa.

## Próximo incremento e rollback

Falta confirmação consumível e Host/Guests reais conjuntamente com monitor serializado e retirada terminal81 de todas as leases. Não habilitar transporte86 antes desse trabalho: observação favorável isolada não conserva autorizaçãoUDP durante toda a sessão. Preparação/revisão privadas podem continuar a ser exercitadas sem abrir transporte.

Rollback operacional chama stopConnect e aguarda o mesmo worker/job; o dono85 decide separadamente stop/reap/EULA. Nenhum callback encerra Java ou restaura configuração. Reverter o checkpoint local não altera bytes/digests/convites anteriores, matriz ou evidência histórica. Nenhuma ação humana é necessária para continuar programação e testes locais deste recorte.
