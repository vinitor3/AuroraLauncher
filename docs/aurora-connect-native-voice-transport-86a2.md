# Connect86A2/B — transporte nativo e retirada terminalUDP

Base cumulativa local: `0bca0ea` (90/91/86A1),07/11/2026. Estado: **PROTÓTIPO / PRECISA DE TESTE**. Sete pais focais executados localmente; os demais gates e regressões continuam em verificação. O aceite integral do [contrato86](aurora-connect-native-voice-host-binding.md) continua pendente. Sem UI, comandoTauri, anúncio35554, mod autenticado, áudio, Java novo ou rede externa.

## Implementação em verificação

`connect/native_voice_host.rs` consome uma única revisão86A1 exata, com digest, operação, geração e binding original revalidados. Não reemite os convites: o Host71 recebe a mesma bijeção entre leases/envelopes originais e serviços compilados. A confirmação registra o ator no mesmo `Attempt.worker`, e observações posteriores reservam o mesmo QueryJob84 antes do dispatch. As APIs de confirmação/exportação são privadas Rust; nenhum DTO remoto seleciona fonte/porta/perfil.

O ator é o único proprietário forte do Host fora de trabalhos81 registrados. O estado do Attempt conserva referência fraca para observação/exportação síncrona; uma revisão ou receipt não mantém Endpoint aberto. Há consulta completa85 antes do bind, depois do bind e conferência final de autoridade/janela antes da publicação. Falha pré-publicação recolhe transporte e aguarda a mesma query.

Após publicar, uma falha de voz ou a janela2s sela o estado terminalUDP e inicia chamadas81 para todas as leases originais antes de aguardar qualquer drenagem. `Retiring` não significa selo atômico ou recursos liberados; `Retired` exige todos os resultados81 `Ok(())`. Os handles e o latch permanecem com o ator, inclusive sem waiter. Amostra favorável posterior não reativaUDP, muda revisão, renova convite ou orçamento.

Uma query bloqueante vencida continua registrada até seu término. Nesse intervalo o ator observa cancelamento e autoridades originais, pode fechar transporte, mas não iniciaTCP paralelo. Depois da retirada só consultaTCP separadamente, serializado, no máximo uma vez por segundo sem catch-up. Busy aninhado mantémTCP sob os gates originais; erros de voz não viram diagnósticoTCP. EncerramentoConnect não chama stop do DedicatedRun nem restauraEULA.

## Fixture causal do mesmo produtor

`voice_transport_fixture.rs` estende somente o filho de teste85. O caminho STATUS existente continua no codec original. Um prefixo fechado, vinculado ao nonce próprio do harness, seleciona echoTCP limitado; UDP aceita datagramas marcados de até4096B com orçamento total64KiB nos dois sentidos. Dois comandos fechados retêm/liberam no máximo quatro respostasUDP reais. A confirmação de retenção significa que o mesmo filho recebeu e validou os payloads; não é contador desconectado do destino.

O primeiro pai novo usa dois GuestsQUIC independentes e duas origensUDP locais por Guest, por bridges efêmeras da biblioteca. Nos dois perfis executados comprovou TCP em progresso enquanto quatro respostasUDP estavam realmente retidas, retorno4096B íntegro/isolado, configuração incompatível→retirada, TCP antigo/novo saudável, configuração restaurada+amostra85 favorável sem reativaçãoUDP, recusa de nova abertura/ponteUDP e cleanup/rebind. Não prova áudio ou clientesMinecraft.

## Gate81 entre crates: decisão explícita de teste

Os gates81 anteriores eram `cfg(test)` privados dentro da biblioteca. O teste nativo desktop, que usa essa biblioteca como dependência, não podia reter causalmente seus owners. Root revisou e aprovou esta extensão local limitada:

- `apps/connect-lab/Cargo.toml`: feature `native-voice-test-support = []` sem dependência externa.
- `apps/desktop/src-tauri/Cargo.toml`: dev-dependency para a mesma crate local já fixada, ativando apenas essa feature em testes.
- Toda implementação/exportação exige **Windows + feature + debug_assertions**. Consumidor desktop exige adicionalmente `cfg(test)`. API ausente sem a feature e nos perfis release convencionais, com `debug_assertions=false`, mesmo selecionando a feature. Um perfil release customizado que habilite deliberadamente `debug_assertions=true` e a feature inclui o suporte; o gate não consulta o nome do perfilCargo.
- `NativeVoiceRetirementGate` opaco/RAII oferece somente entrada, liberação e observações de endereço de socket/timeout. Dois estágios fechados: `AfterConnect` e `Completed`. Não recebe closure/provider nem concede enable, grant ou autoridade.
- O registro guarda `Weak` do gate; o gate não guarda Host, Endpoint, socket, work guard ou payload. A espera ocorre onde o owner81 real ainda conserva esses recursos. Drop libera a mesma espera; fallback8s é o limite histórico de teste e é observado como falha do oracle se ocorrer.
- Uma leitura de teste separada observa o selo real de `Control.enabled`; estado `Retiring` sozinho nunca é usado como prova de que a segunda lease já selou.

Nenhuma versão externa ou lockfile foi alterado. A compilação `--locked --all-features --tests` passou; combinação normal e release ainda precisam confirmar a ausência da API antes da integração. Os testes81 originais e seus hooks não foram substituídos: o suporte novo usa slot separado e vazio quando não instalado.

O segundo pai novo percorreu ambos os perfis e os dois estágios, quatro casos. Manteve o primeiro socket/owner real retido, observou a segunda lease já selada e recusando uma abertura nova, liberou o primeiro owner e exigiu rebind de sua porta exata. Depois perdeu um waiter de stop e exigiu rebind do transporte enquanto a query85 original ainda estava retida; cleanup geral só concluiu após essa query. A portaUDP do filho43002/43003 permaneceu legitimamente ocupada até o dono85 encerrá-lo.

## Evidência executada, pendências e preservação

Root executou serialmente, sem emuladores ou outra fixture nas portas43001–43003:

```text
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --lib native_voice_host -- --test-threads=1
```

Primeiro resultado: **2 pais PASS**, seis casos de perfil/estágio,57.07s de testes após1m37s de compilação. Log local: `.aurora/connect-validation/resume86b-host-first.log`. Depois das correções e da ampliação descritas abaixo, o mesmo comando executou **7 pais PASS,0 ignorados**,18 casos de perfil/gate,169.55s de testes após1m18s de compilação. Log final: `.aurora/connect-validation/resume86b-host-cases-final.log`. Os dois pais anteriores foram repetidos nesse filtro: não são nove pais distintos. A primeira compilação `check --all-features --tests` passou em22.84s; sua única advertência, variante `Reason::Transport` sem uso, foi removida antes do primeiro focal. Estes resultados são próprios de86A2/B; os13 PASS anteriores do filtro86A0/A1 não são evidência deste código posterior.

Os cinco pais/doze casos adicionais de `native_voice_host_cases.rs` passaram no filtro final: Busy externo do mesmo84, stop antes/depois do bind com query real retida, ingresso consumido vencido e ingresso não admitido vencido, janela vencida com resultado real pronto e perdaUDP distinta de perdaTCP. O comando fechado `DropTcp` apenas solta o listener do próprio filho; não termina seu processo ouUDP. As duas recusas de listener exigiram o erro tipado da consultaOS correspondente, não um match sintético isolado.

A revisão independente dos casos encontrou duas lacunas de força do oráculo, corrigidas antes da execução: soltar o slot84 e esperar20ms não prova conclusão do `JoinHandle`; e um handle de thread descartado no unwind não prova seu join. O caso de empate agora exige observação `cfg(test)` de `task.is_finished() && now >= window` imediatamente antes de cada polling do mesmo `select`. Esse observador conserva somente metadados de teste e não altera ordem, resultado ou cancelamento da seleção. A consulta externa usa RAII para liberar o gate e recolher a mesma thread também no unwind. A projeção do ator também passou a recusar geração/revisão divergentes antes de avançar sua geração, evitando que uma atualização de observação sobrescreva invalidação concorrente.

| Gate do contrato86 | Evidência atual e limite |
| --- | --- |
| Origem/revisão/reserva84 |86A0/A1 têm13 pais PASS em filtro sobreposto separado; não substituem os gates posteriores de Host. |
| Bytes/duas origens por Guest/retorno/TCP simultâneo | Primeiro pai86A2 PASS nos dois perfis; bytes opacos próprios. |
| Retirada coletiva/AfterConnect/Completed/rebind/stop com cauda85 | Segundo pai86A2 PASS nos quatro casos; nenhuma conclusão por mapa vazio ou apenas cancelamento. |
| Busy externo, voz versusTCP, stop pré/pós-bind, ingresso consumido e empate de janela | Cinco pais adicionais PASS,12 casos. Empate observado antes do polling do select; consulta externa e waiters perdidos conservam os owners reais. |
| Configuração | Rewrite semântico inválido e restauração sem reativação passaram no primeiro pai. Rename/substituição durante leitura, remoção, limite e unsafe ainda precisam de provas do consumer86 com leases/TCP reais. Testes85 prévios da fonte não fecham esse gate de86. |
| Autoridade e corridas adicionais | Permanecem gates de geração/conta, expiry original antes da publicação, pânico controlado, sessão/revogação individual e todas as autoridades concluídas no caminho86. |
| Compatibilidade e integração |81 passou23/23; padrão desktop298PASS/5ignorados; fmt e Clippy all/default/connect desktop e all-features biblioteca passaram. Comandos/tempos na tarefa92/93. Regressão completaV1/84/85 e combinações release ainda pendentes; não reatribuir goldens antigos a esta rodada. |

Não declarar86 concluído pelos sete pais ou por compilação. As pendências são localmente testáveis dentro do contrato, sem depender de Java, áudio ou intervenção humana. O próximo recorte de configuração foi somente proposto; nenhum código desse recorte entrou neste checkpoint.

Core/Skins/nove pares, Assistente, Recovery/Doctor/CAS, propriedades/EULA, protocolos/convites anteriores, dependências externas, relay e default-off são preservados. Não houve autorização de push/deploy/instalador/billing, firewall ou dados pessoais. Rollback operacional encerra Connect e espera seus owners; o dedicado conserva stop/completion/EULA próprios. Não há intervenção humana necessária para testes locais adicionais.
