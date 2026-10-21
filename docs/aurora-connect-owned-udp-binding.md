# Binding UDP de processo próprio no Windows — contrato82

**PROTÓTIPO**, implementação,19 testes focais, regressão514 PASS, checks e revisão aprovados. Data:21/10/2026. Base de leitura da proposta: `9213cc8`; base de implementação após concluir81: `a819fae2c0a4a7c08c36a370b3e04ae293ca04e3`, branch `codex/aur-r4-connect`. Contrato de SHA256 inicial `87bce0ffc09fb2453b05daf5543ff78c6b8556ce6359703fe28cc95f6850a11a` integralmente aprovado por root e QA antes do código. Este documento não altera o contrato nem atribui resultados81 a82; execução local autorizada sem nova ação humana.

O recorte é uma consulta nativa, interna e somente leitura para exigir a presença de um binding UDP exato associado ao processo original representado por `OwnedChildProof`. Não cria servidor dedicado, processo, ponte, convite, revisão de voz, comando Tauri ou interface. `connect-lab` e `connect-voice-lab` continuam desligadas por padrão; relay público continua desligado.

## Bloqueio concreto e fronteira deste incremento

A descoberta atual em `apps/desktop/src-tauri/src/engine/connect_discovery/windows.rs` conserva um handle duplicado do `Child` próprio, PID, instante de criação e identificadores de instância/run. As funções `listeners` e `require_listener` consultam apenas TCP. O [perfil64](aurora-connect-discovered-session-udp.md) continua usando game/TCP descoberto e aux/UDP manual; a [abertura73](aurora-connect-ready-game-launch.md) é lançamento de cliente em cópia pronta, não um supervisor nativo de dedicado.

O vínculo de voz depende de prova própria de UDP e continua fora deste incremento.82 fornece somente a consulta dessa parte da identidade OS. O [encerramento independente de UDP81](aurora-connect-udp-service-disable.md) é uma operação distinta: consultar um binding não cria nem reativa a autoridade de transporte que81 retirou. A autoridade82 é completa neste contrato e não depende de uma proposta em andamento fora do commit.

`RunningInstances::connect_owned_child`, em `apps/desktop/src-tauri/src/commands.rs`, obtém a prova do filho rastreado e do run já registrado. Preservar esse limite: nenhuma entrada nova aceita PID, handle, instante de criação, instância/run alternativos, executável ou argumento de processo fornecido pelo frontend. Não criar um construtor de `OwnedChildProof` a partir de número de PID.

## API interna implementada

No módulo `engine::connect_discovery`, apenas em Windows com `connect-lab`:

```rust
#[cfg(all(windows, feature = "connect-lab"))]
pub(crate) fn require_owned_udp_binding(
    proof: &OwnedChildProof,
    endpoint: SocketAddr,
) -> Result<(), DiscoveryError>;
```

A implementação Win32 permanece privada, podendo ser chamada por um wrapper ou reexportação de visibilidade restrita. Não adicionar uma API pública de inventário UDP, DTO, campo de snapshot, capability remota, retorno serializável ou novo erro de IPC. A prova emprestada permanece com o caller durante toda a consulta; não há transferência de ownership, substituição, cópia ou cache. O resultado `()` não é um permit ou recibo reutilizável.

`Ok(())` significa: uma amostra limitada da tabela de endpoints da família solicitada continha ao menos uma linha com endereço/porta/escopo exatos e PID do processo original; a prova de processo foi revalidada antes e depois da consulta/decodificação, antes de retornar sucesso. Isso não reserva a porta, mantém o socket aberto ou garante continuidade após a última observação.

| Entrada ou condição | Resultado proposto |
| --- | --- |
| Porta0, endereço fora do loopback literal permitido, wildcard, IPv6 mapeado, scope/flowinfo IPv6 não zero | `DiscoveryError::InvalidRequest`, antes de consultar a tabela. |
| Handle original encerrado/inválido, PID divergente ou instante de criação divergente | `DiscoveryError::ProcessChanged`; nenhum PID substituto é aberto. |
| Tabela válida sem linha exata do PID original, inclusive linha apenas de outro processo ou PID0 | `DiscoveryError::ListenerChanged`. O nome é legado do enum; aqui significa binding ausente/divergente, não listener de voz. |
| Consulta Win32 indisponível/falha ou retorno de status não suportado | `DiscoveryError::Unavailable`, sem fallback por shell, abertura de handles alheios ou varredura adicional de processos. |
| Tamanho/quantidade excedidos, crescimento sem estabilizar no número limitado de tentativas, aritmética inválida ou tabela estruturalmente truncada/inconsistente | `DiscoveryError::Limit`, sem truncar a amostra para aceitar um prefixo. |
| Prova viva e linha exata, após todas as verificações | `Ok(())`, somente observação. |

Validação estática de endpoint precede I/O de consulta. Em caminho de sucesso da tabela, a rechecagem posterior da prova ocorre antes da decisão final sobre a linha encontrada; se o filho terminou durante a consulta, não devolver sucesso com o snapshot antigo. Falha anterior de consulta ou decodificação já nega o resultado e não precisa produzir uma causa de término mais específica.

## Identidade preservada e limites de instância/run

Reutilizar a prova existente: `from_child` parte de um `std::process::Child` possuído pelo caller confiável; `DuplicateHandle` produz handle próprio não herdável; `creation` confere vida do processo, PID e `GetProcessTimes`. A consulta nova usa esse mesmo handle. Não usar `OpenProcess` por PID, procurar outro Java nem adotar um processo que reabriu o mesmo endpoint.

Os campos `instance` e `run` são metadados privados congelados quando o caller confiável constrói a prova. Sua associação com a execução vem desse caller, não da tabela UDP.82 não lê o banco Doctor, a identidade de Recovery, a árvore da cópia ou a configuração de voz. A assinatura de dois argumentos não permite comparar um par externo de instância/run nem detectar que um caller confiável rotulou incorretamente o filho na construção. Não afirmar essa validação inexistente.

As consultas e `duplicate` preservam os identificadores originais; um futuro vínculo deve comparar essa identidade à cópia/run e revisão que já possui. Nenhum retorno novo oferece strings que o frontend possa reapresentar como prova. Um handle retido após término continua apontando para o processo original; uma linha de outro filho no mesmo endereço, mesmo que ocorra reutilização de PID, não torna a prova original viva.

## Endpoint exato, sem normalização que amplie a origem

Aceitar IPv4 cujo endereço literal passa por `Ipv4Addr::is_loopback`, mantendo seus quatro octetos exatos, ou IPv6 exatamente `::1`, com `scope_id=0` e `flowinfo=0`. Porta deve ser não zero. `127.0.0.2` não é substituído por `127.0.0.1`; família e endereço devem coincidir com a linha observada.

Recusar `0.0.0.0`, `::`, endereço externo/LAN, IPv6 mapeado como `::ffff:127.0.0.1`, DNS e valores de configuração herdados/não resolvidos. `SocketAddr` não recebe DNS; qualquer parser futuro continua responsável por não introduzir resolução. Não reutilizar `local_target`, que no inventário TCP existente permite projetar wildcard como loopback.82 não muda esse comportamento TCP nem acrescenta essa projeção ao UDP.

Uma linha wildcard não satisfaz uma solicitação literal loopback por compartilhar a porta. Uma linha de outra família também não satisfaz o pedido. O caso mapeado já observado nos laboratórios de voz permanece dependência separada, exigindo contrato e prova próprios antes de ser aceito; não ampliar82 por inferência.

## Consulta Win32 e decodificação limitada

Usar `GetExtendedUdpTable` com `UDP_TABLE_OWNER_PID`, `Reserved=0` e somente a família correspondente ao endpoint (`AF_INET` ou `AF_INET6`). A API distingue as estruturas de retorno por família e informa o espaço necessário quando a consulta recebe buffer insuficiente. Não fazer consultas de módulo/processo completo nem enviar datagramas para verificar o resultado. [Documentação Microsoft da função](https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-getextendedudptable).

1. Fazer a consulta de tamanho documentada e no máximo duas tentativas de preenchimento. Crescimento repetido encerra com `Limit`; nenhum loop ilimitado de alocação ou espera. Liberar o buffer anterior antes de substituí-lo.
2. Limitar a tabela a1MiB e16384linhas. Armazenamento deve estar alinhado para DWORD, por exemplo `Vec<u32>`, com arredondamento e multiplicações conferidos. A capacidade real da alocação e o tamanho comunicado à API não podem ultrapassar o teto.
3. Conservar o comprimento em bytes devolvido pela chamada bem-sucedida junto com o buffer. Ele não pode exceder a capacidade fornecida. Toda validação de header, offset e linhas usa esse comprimento, não o tamanho arredondado da alocação. Bytes extras zerados de capacidade não podem completar uma linha truncada.
4. Calcular início das linhas com `offset_of!(MIB_UDPTABLE_OWNER_PID, table)` ou `offset_of!(MIB_UDP6TABLE_OWNER_PID, table)` e tamanho da linha com `size_of` da struct SDK correspondente. Não supor header4, ausência de padding ou offsets codificados à mão.
5. Antes de ler linhas, validar header completo, contador, `count * row_bytes + offset` com `checked_mul`/`checked_add` e limite no comprimento retornado. Conferir a faixa de cada leitura. `read_unaligned` pode ser usado depois da validação; o buffer alinhado não dispensa a prova dos limites.
6. Tabela válida com zero linhas resulta em binding ausente. Uma tabela inválida não é aceita por conter uma primeira linha aparentemente favorável. Linhas de outros PIDs não são exportadas, registradas em log ou convertidas em novos candidatos.

`dwLocalAddr` IPv4 é decodificado no formato `in_addr`, preservando os octetos; `dwLocalPort` usa a conversão de porta de16bits em ordem de rede. `dwOwningPid=0` não identifica o proprietário e nunca concede sucesso. [Estrutura IPv4 oficial](https://learn.microsoft.com/en-us/windows/win32/api/udpmib/ns-udpmib-mib_udprow_owner_pid).

Em IPv6, `ucLocalAddr` fornece os16octetos. `dwLocalPort` também está em ordem de rede e `dwLocalScopeId` requer conversão de32bits da ordem de rede para a representação do host; não copiar o escopo sem conversão por analogia com o leitor TCP existente. O recorte aceita apenas escopo0, mas o decoder deve ser testado com valor não simétrico para verificar a conversão. [Estrutura IPv6 oficial](https://learn.microsoft.com/en-us/windows/win32/api/udpmib/ns-udpmib-mib_udp6row_owner_pid).

Os helpers TCP atuais validam contagem contra a capacidade do `Vec`; isso não é modelo suficiente para o contrato de comprimento retornado de82. Preferir helpers UDP estreitos e revisão própria. Não refatorar o inventário TCP inteiro incidentalmente nem alterar a política de descoberta48/64/74.

A consulta é síncrona e não cria worker, socket, canal ou listener. O limite de tentativas/memória não é promessa de duração máxima de uma chamada do sistema operacional. Um futuro caller assíncrono deve registrar o trabalho sob seu lifecycle antes de deslocá-lo para execução bloqueante; perder a future do caller não pode ser tratado como prova de término. Essa integração não está em82.

## Significado limitado do resultado

A documentação Microsoft explica que uma aplicação pode fazer bind UDP apenas para enviar, sem pretender receber. Portanto, usar “binding observado do processo próprio”. Não usar “listener de voz pronto”, “mod autenticado”, “dedicado saudável” ou “áudio disponível”. [Limite descrito na estrutura UDP](https://learn.microsoft.com/en-us/windows/win32/api/udpmib/ns-udpmib-mib_udprow_owner_pid).

A tabela e as rechecagens são snapshots. Não tornam atômicos observar a linha e o próximo acesso, não fornecem identidade de socket físico, intenção do aplicativo, exclusividade da porta, configuração de reuse, entrega de pacotes ou associação entre o socket e determinado JAR. Outros processos podem mudar seus próprios sockets durante a observação;82 não os interrompe.

Se o mesmo processo fecha e reabre o mesmo endpoint entre observações, a tabela pode voltar a produzir `Ok(())`. Isso não demonstra continuidade do socket anterior. Se a ausência for observada, o resultado é erro naquele momento;82 não mantém estado de revogação permanente. Um futuro vínculo invalidado e um serviço desativado por81 não são reabilitados por uma observação posterior favorável.

Revalidar o handle antes/depois reduz a possibilidade de aceitar uma linha de um processo já encerrado, mas não impede o processo de terminar imediatamente após a última consulta. Não prometer eliminação de toda corrida local nem usar o resultado como autorização permanente.

## Provas locais exigidas — resultados nos checkpoints abaixo

As fixtures usam apenas filhos próprios do binário de teste, endpoints efêmeros loopback e comandos de controle por stdin. Não executar Java, abrir relay, baixar artefatos ou consultar processos pessoais para obter um caso positivo. O padrão do filho em `engine/connect_discovery/tests.rs` pode ser reaproveitado por referência; não mudar os testes TCP existentes sem necessidade demonstrada.

| Grupo | Prova necessária e oráculo |
| --- | --- |
| Decoder puro | IPv4/IPv6 com endereço, porta não simétrica e scope de32bits; zero linhas; PID0 e PID diferente; truncamento de header/linha; contador acima de16384; tamanho acima de1MiB; overflow; comprimento retornado menor que alocação preenchida; família/offset corretos. Rejeição ocorre antes de leitura fora da faixa. |
| Resposta Win32 limitada | Seam privada `cfg(test)` para tamanho/growth/status: insuficiência seguida de sucesso, duas tentativas ainda insuficientes, tamanho excessivo e sucesso com comprimento contraditório. Nenhuma injeção de provider pública ou em produção. |
| Filho vivo próprio | Filho cria sockets UDP distintos em `127.0.0.1:0` e `[::1]:0`; publica endpoints realmente atribuídos em saída limitada. Criar a prova do `Child` original; as duas consultas exatas passam. Um bind sem loop de recepção pode passar e deve continuar descrito apenas como binding. |
| Endpoint negativo | Porta0, wildcard, endereço não loopback, mapped, scope/flowinfo inválido, outra porta e endereço loopback diferente são recusados. Se a fixture cria wildcard para um negativo, a consulta literal loopback à mesma porta não passa. |
| Proprietário diferente | Socket mantido pelo processo pai não satisfaz a prova do filho. Endpoint de segundo filho não satisfaz a prova do primeiro; cada prova válida pode reconhecer somente o binding de seu próprio filho. Preservar instância/run ao consultar e duplicar a prova. |
| Release/rebind no mesmo filho | Comando por stdin libera o socket e confirma a ação; consulta retorna `ListenerChanged`. Rebind explícito do mesmo filho ao mesmo endpoint pode voltar a passar, documentando a ausência de identidade física/continuidade. Não inferir reativação de transporte. |
| Processo substituído | Encerrar e recolher o filho original mantendo sua prova; outro filho ocupa o mesmo endereço exato. A prova antiga retorna `ProcessChanged`, a nova reconhece o novo binding. Não depender de provocar reutilização real de PID. |
| Término durante consulta | Gate privado determinístico retém a fase entre primeira rechecagem e retorno da tabela; terminar o filho original, liberar a consulta e exigir recusa na rechecagem posterior. Controle de teste não pode mudar a autoridade de produção. |
| Cleanup real | Enviar stop normal, aguardar saída e readers/threads próprios; só usar fallback de término no próprio filho se ele exceder o deadline da fixture. Soltar pipes/handles próprios e comprovar rebind de todos os endpoints controlados. Falha de asserção também conserva um owner de cleanup que recolhe o filho. |

O handshake de fixture deve reconhecer comandos fixos (`release`, `rebind`, `stop` ou equivalentes), limitar bytes/linhas e não executar conteúdo recebido como shell. O filho pode receber um endpoint escolhido pelo pai somente nesse protocolo privado de teste, para reproduzir o mesmo endereço após troca de processo; a API de produção continua sem lançamento ou escolha de PID.

IPv6 loopback deve ser realmente exercitado para atribuir cobertura IPv6. Se o ambiente não o disponibilizar, registrar essa limitação sem converter ausência em PASS ou substituir o caso por IPv4 mapeado. Não alterar Windows, firewall ou configuração da rede para forçar um resultado.

Root coordena os comandos de validação após liberar a implementação82: focais Windows, regressões da descoberta TCP e checks padrão/`connect-lab` pertinentes, com fmt/clippy e QA independentes. Não executar opt-ins históricos, probes Java reais ou testes externos por seleção ampla. Resultado de82 não soma nem substitui os seis testes públicos de81 e suas provas privadas.

## Primeiro checkpoint82 — produção e decoder

Implementados wrapper interno em `connect_discovery/mod.rs`, novo `connect_discovery/udp.rs` e11 testes em `connect_discovery/udp/tests.rs`. Windows/TCP existente intacto. A anotação estreita de dead_code do wrapper documenta a ausência de caller de produção neste recorte; não foi criado uso falso em UDP manual/VoiceMutationGuard nem nova API pública. Query e hook de pós-decode substituíveis existem somente cfg(test), sem provider de produção.

Comandos executados pelo root após freeze dos três inputs registrados:

```powershell
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --features connect-lab --lib
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --features connect-lab --lib engine::connect_discovery::udp::tests -- --test-threads=1
```

Check **PASS**,6,79s (`.aurora/connect-validation/owned-udp-binding-82-first-check.log`). Focal **11 PASS/0 falhas/0 ignorados**,0,00s/compilação50,86s, exit0 (`owned-udp-binding-82-unit-first.log`), sem avisos. Política exata de endpoint, octetos/porta/PIDv4 e scope/portav6, tabela vazia, truncamento com primeira linha favorável, comprimento retornado versus alocação, tetos, overflows e scripts de chamadas Win32/growth/status. Não executa ainda binding positivo real do sistema, lifecycle de filho ou áudio.

Root e QA independente leram a produção e os oráculos sem bloqueador: buffer alinhado/limitado, tabela inteira validada antes de correspondência, reads somente de structs SDK de inteiros/octetos dentro dos limites, recheck do proof original antes/depois inclusive sem match. O arquivo isolado de testes Child está em escrita separada e ainda não foi registrado/compilado neste checkpoint. Estado **PARCIAL**; faltam esses testes reais e regressões/fmt/clippy finais.

## Segundo checkpoint82 — processos e Win32 reais

Registrado o novo `connect_discovery/udp/owned_process_tests.rs` depois de seu freeze separado. Comando do root: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --features connect-lab --lib engine::connect_discovery::udp:: -- --test-threads=1`. Resultado **19 PASS/0 falhas/1 ignorado**,0,35s/compilação28,61s, exit0 (`owned-udp-binding-82-real-first.log`), sem avisos. São11 unitários anteriores e oito casos reais; o helper ignorado é invocado internamente pelos pais com seleção exata e não conta como20PASS. Não há fallback que omita IPv6.

Filhos próprios ocultos (CREATE_NO_WINDOW), socketsV4/V6 reais, protocolo stdin JSON fixo com limites e owner RAII instalado imediatamente depois de spawn. Rodadas confirmaram endpoint literal/exato, identidades/duplicate preservados, rejeição de wildcard/parent/outro filho, release/rebind pelo mesmo processo e prova antiga recusada quando novo filho ocupa o mesmo endereço. Callbacks sem provider de query capturam tabela Win32 real positivaV4/V6 ou negativaV4, encerram/recolhem o filho antes da rechecagem final e exigem ProcessChanged. Catch_unwind comprova stop gracioso, reader recolhido/sem fallback e rebind das portas depois do unwind.

Root e QA independente leram API/decoder, ownership de Child/readers/protocolo e os oito oráculos; conferiram fonte e log sem bloqueador. Nenhum dado de processo alheio é exportado. Essa evidência local comprova binding/identidade amostrados, sem loop de recepçãoUDP, mod, Java, Minecraft, voz, GUI ou internet. Fmt/Clippy/regressões ainda pendentes neste checkpoint.

## Verificação final82

Fonte final congelado antes de cada Cargo, executado somente pelo root:

```powershell
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --all-targets -- -D warnings
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-targets -- -D warnings
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --features connect-lab --all-targets -- -D warnings
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --all-targets -- --test-threads=1
```

Fmt **PASS**; Clippy **PASS**15,64s/5,59s/15,15s nas configurações acima. Regressão **514 PASS/0 falhas/64 ignorados**, oito targets,251,09s somados de testes/compilação1m46s, exit0/sem avisos. Logs privados `owned-udp-binding-82-{fmt,clippy-all,clippy-default,clippy-connect,all-full}.log`. Inclui19 novos testes e regressõesTCP/Recovery/Doctor; helper novo ignorado é executado somente pelos pais de sua fixture e não se soma ao PASS. Demais ignorados históricos continuam sem nova execução. Suíte padrão runtime290 pertence a80 e biblioteca295 a81; não repetidas nem atribuídas a82. Sem frontend alterado, Node/build/Java não foram executados82.

Arquivos: quatro Rust `engine/connect_discovery/{mod.rs,udp.rs,udp/tests.rs,udp/owned_process_tests.rs}` sob `apps/desktop/src-tauri/src`; este domínio, task, AGENTS e quatro referências canônicas. QA independente conferiu fonte, limites unsafe, ownership/readers, oráculos e logs. Nenhum bloqueador identificado. Manifests/locks/versões e nove pares inalterados; Windows/TCP anterior intacto.

| Domínio | Estado após82 |
| --- | --- |
| TCP | **PROTÓTIPO**, regressões existentes aprovadas; sem nova API/transporteTCP. |
| UDP | **PROTÓTIPO**, nova observação OS exata somente leitura; transporte/retirada81 preservados. |
| Direto | Sem nova topologia; V4/V6 loopback neste PC. Não amplia evidência externa histórica. |
| Relay | Público desligado; nenhum uso/conta/custo. |
| Minecraft | Sem Java, login ou mundo novos; produtor dedicado nativo pendente. |
| Convites/interface | Sem novo comando/campo/interface; regressões nativas preservadas, sem GUI nova. |
| Simple Voice Chat | Vínculo em execução **NÃO IMPLEMENTADO**; áudio **PRECISA DE TESTE**; consulta não identifica o mod. |
| Plasmo Voice | Vínculo em execução **NÃO IMPLEMENTADO**; homologação separada de áudio **PRECISA DE TESTE**. |

## Integrações que continuam pendentes

O futuro produtor de dedicado precisa iniciar e supervisionar um processo próprio por fluxo adequado, conservar cópia/run/handle, tratar termos/EULA no contexto correto e correlacionar game/TCP com aux/UDP da mesma execução.82 não introduz esse produtor nem reutiliza o lançamento de cliente73 como se fosse servidor.

Leitura da configuração em execução, pins SVC/Plasmo, identidade física da cópia, mod selecionado, digest/revisão do vínculo, participante local que ocupa uma das oito vagas, monitor de mudanças e coordenação com81 continuam em contratos próprios. Não inferir destino de voz a partir de qualquer porta que o processo possua.

`VoiceMutationGuard` permanece exigindo ausência de java/javaw nas mutações e suas rechecagens existentes. A consulta82 não chama a inspeção59 com um guard enfraquecido, não modifica arquivos e não autoriza aplicar/restaurar anúncio com Java aberto. Recibos RAM/duráveis58/60/67/68, restauração estrita/reconciliada, aposentadoria69/70, Recovery/CAS e Doctor conservam suas autoridades. Preservar o Assistente desktop, alpha.14 e os nove pares Core/Skins.

## Aceite, rollback e estado final permitido

Aceite82 exige a API interna descrita, prova original antes/depois, correspondência exata sem normalização ampla, decoder limitado que respeita comprimento retornado e testes locais causais de processo/binding/cleanup. Não basta compilar, obter um PID igual ou consultar uma porta atualmente ocupada. Nenhum resultado pode expor linhas de processos alheios, fingerprints privados ou novos poderes ao frontend.

Resultado final **PROTÓTIPO de observação de binding UDP próprio no Windows**. Minecraft, mod, dedicado, GUI, áudio e redes externas não são homologados por esse recorte. Checkpoints anteriores preservam seus estados históricos, sem somar rodadas sobrepostas.

Rollback de código: reversão local revisada apenas82, preservando81/80 e os demais contratos. Não existe migração, arquivo pessoal, recibo ou configuração a restaurar. A consulta não mata o processo observado, fecha seus sockets nem desfaz uma decisão anterior de transporte. Cleanup das fixtures atua somente nos filhos que elas criaram.

Nenhuma ação humana é necessária para implementar, revisar e testar esse recorte local autorizado depois da coordenação técnica. Push, deploy, release, instalador, cobrança, Java e ensaios externos continuam fora do escopo.
