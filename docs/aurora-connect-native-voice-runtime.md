# Runtime nativo de voz em cópia privada — contrato85

**CONTRATO APROVADO / 85A PROTÓTIPO / 85B PARCIAL.** Data: 21/10/2026. Root e QA leram e aprovaram integralmente o texto SHA256 `51a2f28fb992931c995246e1125ec2464d5bf783223b1176e5a4c654fc37f895` antes do código. O gate prévio foi concluído:84C revisado e commitado em `c128a215532dd0b4a0254155ed29466d1b9f137e`, full pós-correção605 PASS/0 falhas/68 ignorados, opt-in real STATUS/cleanup pós-correção1 PASS. A revisão restritiva85A foi aprovada root/QA no SHA256 `aea8de2d634edf1260688fc781f4f3a12f9759703209d024f87a0fb9d213eba6` antes do patch corretivo. PrecisãoB0/B1 aprovada antes do código no SHA256 `f0393bc135a4203b40b66eef4d16404ed621ba87a6ecd258b4c10bd03cc51526`. Nenhum resultado ou implementação85 é atribuído aos testes84; registro separado ao final e no task.

Entrega proposta: iniciar, separadamente, dois perfis fechados de dedicado com os mods já fixados; conservar o mesmo proprietário nativo83; observar configuração de execução e binding UDP desse processo por uma operação nativa somente de leitura. A observação é útil para o futuro vínculo de serviço, mas não cria esse vínculo, ponte, convite, canal de áudio ou nova interface. `Ready` continua significando somente o STATUS de jogo definido em84.

Tudo permanece Windows, `connect-lab` + `connect-voice-lab`, desligadas por padrão. Relay público permanece desligado. Não há alteração de IPC, frontend, Java/Core/Skins, matriz, credenciais, configurações pessoais ou protocolo dos mods. O trabalho local já autorizado não exige nova ação humana; a coordenação técnica dos checkpoints e a revisão deste contrato continuam necessárias.

## 1. Documentos e autoridade que este recorte conserva

Leitura obrigatória antes de implementar:

- [SPEC](engineering/AURORA_CONNECT_SPEC.md), [backlog canônico](engineering/CODEX_TASKS/AUR-R4-CONNECT-BACKLOG.md), [MASTER_ROADMAP](engineering/MASTER_ROADMAP.md) e o prompt consolidado `AUR-R4-connect-v2.md`, lido integralmente na worktree documental `codex-aur-r4-connect-v2-docs/docs/engineering/CODEX_PROMPTS/`. O [backlog antigo](engineering/AURORA_CONNECT_BACKLOG.md) é apenas ponte de IDs, não uma segunda matriz.
- [Perfis e endereçamento dos mods](aurora-connect-voice.md) e [runtime de laboratório](aurora-connect-runtime.md): pins, configuração real de cada mod, preparação e ensaios Node históricos. Relatório Node, PID ou porta não se tornam autoridade nativa.
- [Produtor dedicado84](aurora-connect-dedicated-status.md), [proprietário de processo83](aurora-connect-native-owned-run.md), [consulta UDP82](aurora-connect-owned-udp-binding.md) e [retirada independente de UDP81](aurora-connect-udp-service-disable.md).
- [Origem descoberta TCP + UDP manual64](aurora-connect-discovered-session-udp.md), [grupo71](aurora-connect-multiparticipant-session.md), [desktop do grupo](aurora-connect-multiparticipant-desktop.md) e [abertura de cliente73](aurora-connect-ready-game-launch.md).
- [Controlador58](aurora-connect-voice-restoration-controller.md), [inspeção59](aurora-connect-voice-inspection.md), [desktop60](aurora-connect-desktop-voice.md), [recibos duráveis67](aurora-connect-durable-voice-receipts.md), [desktop durável68](aurora-connect-desktop-durable-voice.md) e aposentadoria [69](aurora-connect-voice-receipt-retirement.md)/[70](aurora-connect-desktop-receipt-retirement.md).
- [Cópia diagnóstica](diagnostic-lab.md) e [Recovery](instance-recovery.md): identidade física, observação estrita sem inicialização, locks e limites de cópia permanecem aplicáveis.

A proposta antiga de vínculo de voz, rascunho local não versionado escrito sobre base75 e não implementado, é referência histórica de problemas. Não é contrato canônico de85, não autoriza suas APIs e não será promovida ou reescrita por este incremento.

## 2. Bloqueio real encontrado nas fontes

84 cria uma origem/cópia diagnóstica privada, valida nove artefatos de jogo e JDK, exige `mods` vazio, revisa uma receita fechada, aplica EULA por journal próprio, inicia83 e observa TCP/STATUS do processo original. Seus `Manifest`, contagens de recursos e digest pertencem ao perfil STATUS sem mods. Acrescentar um JAR depois da revisão invalida essa autoridade.

82 observa endpoint UDP literal de um `OwnedChildProof`; recusa IPv4 mapeado.83 já possui esse proof e um único slot compartilhado de consultas TCP/UDP.84 mantém o `OwnedRun`, sua completion original e a transação EULA no mesmo worker; sua fonte TCP é fraca. Esses mecanismos permitem acrescentar observação de voz sem entregar `Child`, proof, PID, completion ou permissão de restauração a outro proprietário.

O parser/inspeção59 é uma operação com `VoiceMutationGuard`: exige ausência conservadora de java/javaw antes/depois do lock. Não é uma API de leitura em execução. Chamá-lo com Java vivo, esconder esse Java do probe ou afrouxar o guard seria alterar a fronteira de segurança.85 precisa de leitor separado, realmente somente de leitura, para os caminhos fechados da cópia que acabou de criar.

Os boots Node históricos de SVC/Plasmo registraram, neste Windows, representação UDP `::ffff:127.0.0.1`; isso não prova qual representação o novo produtor observará. O contrato82 estrito não pode aceitá-la por normalização silenciosa. A política adicional de85 está especificada separadamente na seção6 e deve ter provas próprias antes do ensaio Java.

## 3. Perfis fechados e recursos existentes

Os dois perfis usam Minecraft1.21.1, Fabric0.16.14, Java21/x64/Windows e protocolo de jogo767, dentro do par já usado pelos laboratórios. Não instalar os dois mods juntos, não adicionar Core/Skins ao servidor e não ampliar os nove pares do produto. O perfil STATUS84 permanece sem mods e com exatamente seus artefatos anteriores.

| Perfil85 | Extra compilado sobre os nove artefatos84 | Tamanho | SHA-256 |
| --- | --- | ---: | --- |
| Simple Voice Chat | `mods/voicechat-fabric-1.21.1-2.6.22.jar` | 5.369.475 | `972a01592f670aa57f2dc2e3587a656b798106037468c61e4bfb49eba7d6ae36` |
| Plasmo Voice | `mods/plasmovoice-fabric-1.21.1-2.1.17.jar` | 12.365.966 | `46e1a4ae51786457712991ba05bf573e2d11331aeadb63b7d108839def022a2b` |
| Dependência somente Plasmo | `mods/fabric-api-0.116.10+1.21.1.jar` | 2.425.988 | `af0eb792940568fdb1f37bb65a2599773bc47d2010aedf5ec5ed0bd3b6fc40e6` |

Esses três arquivos já existem nos estágios privados `prepared-Ll1oDQ` e `prepared-6jQLQp`. A auditoria85 recalculou tamanhos/hashes e leu apenas `fabric.mod.json` limitado, sem extrair/executar bytecode. Metadados observados: SVC `voicechat`/`1.21.1-2.6.22`, licença `All Rights Reserved`; Plasmo `plasmovoice`/`2.1.17`, `LGPL-3.0`; Fabric API `0.116.10+1.21.1`, `Apache-2.0`. A presença local não concede redistribuição;85 não publica nem baixa JARs. A tentativa de consultar os endpoints Modrinth de versões pelo navegador não retornou conteúdo; não é uma nova verificação remota nem motivo para substituir os pins existentes.

Conservar os nove pins de84 em `resources.rs`/`pins.mjs`, JDK selecionado e validado, executável/release, receita/classpath/env compilados, `online-mode=false` explicitamente autorizado no laboratório e EULA/MSA já aceitos. A seleção de stage/JDK é do caller Rust confiável; Node JSON, mods/config presentes no stage e caminhos internos de manifesto não escolhem recursos. Copiar somente a lista exata do perfil. Recursos faltantes, extras instalados, hash/identidade alterados, perfil misturado ou metadata incompatível recusam antes de EULA/spawn.

Contagens fechadas propostas: STATUS conserva nove instalados e onze recursos observados, incluindo os dois arquivos JDK; SVC tem dez instalados e doze recursos; Plasmo onze instalados e treze recursos. Os seis registros de proveniência continuam fechados. Configuração mutável de voz é registrada separadamente dos JARs imutáveis, nunca acrescentada a essa lista como pin retido durante Java.

### Configuração criada somente na cópia realmente nova

Factory cria os seguintes baselines compilados, UTF-8/LF, por CreateNew e verificação dos bytes, depois da criação diagnóstica real. Não copia a configuração já usada dos estágios e não aplica anúncio35554. Mudança dessa criação exige novo plano/digest do perfil.

O baselineV2 deve corresponder ao **template compilado do perfil selecionado** e à **mesma Observation concretamente aprovada**: identidade física, tamanho e hash dos bytes efetivamente lidos. Um hash autodeclarado no manifesto, um arquivo apenas semanticamente parecido ou uma nova captura não substituem essa comparação. `server.properties` conserva o template compilado84 e a mesma observação aprovada; configuração de voz acrescenta sua observação própria, sem recapturar nenhuma delas como nova autoridade.

SVC: `config/voicechat/voicechat-server.properties`,176 bytes, SHA-256 `820f42e6cc7043d23b18daf384f5cbbc98fb1c58729cd74b54ae63a425f39e9b`:

```properties
# Disposable Aurora Connect Simple Voice Chat server lab.
# An explicit loopback bind avoids the mod fallback to a wildcard bind.
port=43002
bind_address=127.0.0.1
voice_host=
```

Plasmo: `config/plasmovoice/server/config.toml`,164 bytes, SHA-256 `2dfc96bc221060b2f6cb5ebc73a465893594ea036145fc14d54549bbd2d5634e`:

```toml
# Disposable Aurora Connect Plasmo Voice server lab.
debug = false
check_for_updates = false
use_crowdin_translations = false

[host]
ip = "127.0.0.1"
port = 43003
```

Bind e anúncio são conceitos distintos. A documentação SVC descreve `voice_host` como destino comunicado aos clientes; não é prova da porta efetivamente possuída. A configuração Plasmo distingue `[host]` de `[host.public]`.85 conserva o anúncio padrão/ausente acima, não deduz o destino recebido por um cliente e não declara modo misto funcional. [Configuração SVC](https://modrepo.de/minecraft/voicechat/wiki/server_config), [configuração avançada Plasmo](https://plasmovoice.com/docs/server/advanced/).

Essa criação inicial faz parte de uma cópia privada nova e revisada. Não é permissão para escrever configuração de instância existente com Java aberto. Depois do spawn,85 não escreve, restaura ou reconcilia a configuração de voz produzida pelo mod. O guard dos fluxos58/59/60/67/68 permanece inalterado.

## 4. Compatibilidade dos manifests e journals84

Este é requisito de aceite antes da primeira integração de85. Adicionar `profile` com default ao `Manifest` atual e reserializar schema1 pode mudar o digest já gravado no claim84. Não fazer migração implícita, recalcular o claim antigo ou tratar incompatibilidade como autorização para reconstruí-lo.

Proposta escolhida:

1. Conservar `ManifestV1` com o schema1, campos, ordem de serialização, validação e domínio de digest atuais. Renomear o tipo internamente, se necessário, não muda sua representação. Nenhum campo novo, envelope, tag, `null` ou default é emitido paraV1.
2. Criar `VoiceManifestV2`, schema2, com perfil enum fechado, identidade física lab/bundle/native/cópia/origem, runtime, listas exatas de recursos/proveniência, identidade/hash/bytes da configuração inicial e revisão da política de observação. Não aceitar schema2 STATUS, perfil desconhecido ou campos extras.
3. Um wrapper privado faz dispatch do schema em JSON limitado e desserializa estritamente a estrutura correspondente. Rejeitar chaves duplicadas, inclusive `schema` e duplicatas em objetos internos, schema desconhecido e campos desconhecidos. O dispatch deve preservar os bytes/tokens originais para a validação estrita: não passar por `serde_json::Value` ou mapa que colapse duplicatas e depois reencodar antes do derive. Uma leitura preliminar limitada pode selecionar o decoder, mas a estrutura inteira original continua sujeita à rejeição de duplicatas e campos extras. A serialização encaminha diretamente a estrutura interna; não usa representação externa de enum que acrescente bytes aV1. Schema1 com `profile`, mesmo nulo, é inválido. Não converterV1 paraV2 ao abrir.
4. Preservar o endereço privado `native-status-<uuid>` e sua seleção fechada por `BundleId`; o nome não constitui identificação do perfil. Validação escolhe o conjunto de arquivos pelo schema/perfil persistido, nunca por enum reapresentado pelo caller. Cada bundle é novo e consumível uma vez; não importar/reutilizar journal ou cópia84 como85.
5. O claim85 liga o schema2, perfil, arquivos efetivamente aprovados, receita e digest de execução. EULA conserva os slots/fases/identidades de84, inclusive os estados incompletos que exigem recuperação. A leituraV2 deve funcionar também no recovery fechado; restore depois de crash continua adquirindo internamente o guard real de ausência de Java e uma revisão fresca. Nenhum parâmetro `may_have_spawned`, report ou completion externo concede restauração.

A observação da configuração inicial emV2 documenta os bytes aprovados antes do spawn. Não exigir que o arquivo produzido pelo mod ainda tenha esses bytes/fileID para reabrir o bundle ou restaurar EULA após execução: validar estruturalmente o registro inicial e conservar a autoridade original de EULA, seus próprios slots e a proveniência imutável. Configuração de voz divergente faz a observação de voz falhar; não transforma o baseline inicial em requisito impossível de cleanup nem autoriza restaurar a configuração. Tests de reescrita real devem exercer também reopen/recoveryEULA, sem tocar no arquivo de voz reescrito.

Provas obrigatórias: golden de bytesV1 efetivamente persistidos antes da mudança, claim/manifest hash e digests idênticos antes/depois de decode/encode, execução/review STATUS inalterados e recovery de todos os prefixos84 com zero trocas indevidas. Não produzir o golden apenas pelo encoder novo. Testar V2, `schema` duplicado com valores iguais e diferentes, chaves duplicadas em cada objeto relevante, schema desconhecido, perfil alterado, V1 com campo extra, downgradeV2→V1, alias/root transferido e conteúdo reencodado. O caso duplicado precisa alcançar o dispatch real, não somente o decoder isolado. Um binário antigo não passa a entenderV2; rollback de código só ocorre depois de concluir/preservar de forma explícita os journalsV2, nunca pelo binário antigo tentando reinterpretá-los.

## 5. API mínima proposta e revisão de execução

Precisão dos checkpoints85B, antes de sua implementação: começar com85B0 somente de compatibilidade/captura, preservando13 prefixosV1 nos mesmos caminhos/IDs, sem `mem::forget` ou handles retidos entre execuções. Retenção é opt-in explícito de TestRoot, defaultfalse, e Drop normal fecha pins/guards. Capturar bytes/preimagens efetivamente anteriores aV2 para Manifest/Claim/ApprovedFiles/Factory/Recipe, com UUIDs originais fixos; replay usa o mesmo encoder/preimagem da produção, não hash(raw) ou encoder antigo duplicado só para testes. Nova revisão recovery tem outro UUID legitimamente. Snapshot/Stage continuam NeedsRecovery e intactos. Índice privado é dado de seleção de fixtures próprias, nunca autoridade para restore; a verificação usa lab/BundleId original e guard/revisão novos. Divergência preserva evidência; cleanup somente da raiz própria confinada e revalidada, sem mover ou relabelar o bundle.

85B1 separa somente o codec. ManifestV1 mantém exatamente os14 campos/ordem/atributos atuais; ApprovedFiles permanece totalmente intocado até85B2. Wrapper privado `Manifest::{StatusV1, VoiceV2}` serializa diretamente o inner e oferece accessors explícitos, sem Deref/flatten/untagged. `read_manifest` lê no máximo32KiB uma vez; `decode_manifest(original_bytes)` faz SchemaProbe e derive estrito sobre os mesmos bytes. B1 reconhece o formatoV2, mas Bundle/reopen/validate e planejamentoSTATUS ainda recusam kindV2 antes de pins de arquivos, guard, Recovery/EULA ou receita.85B2 só pode liberarV2 depois da validação física completa.

FormatoV2 exato e ordem compilada propostos: `schema=2`, `profile="svc"|"plasmo"`, `id`, `lab_identity`, `bundle_identity`, `native_identity`, `copy_id`, `copy_uuid`, `origin_uuid`, `copy_identity`, `origin_identity`, `origin_fingerprint`, `resources`, `installed`, `provenance`, `runtime`, `initial_voice_configuration`, `observation_policy=1`. Runtime é objeto fechado na ordem minecraft/loader/loader_version/protocol, valores1.21.1/fabric/0.16.14/767. Initial_voice_configuration é objeto fechado name/observation; name deve igualar o caminho compilado do perfil e jamais selecionar caminho por JSON, e observation conserva identity(volume/index)/bytes/sha256. Policy1 é internamente LiteralOnlyFirstPersisted, coerente com a restrição85A. Perfil persistido usa enum privado fechado diferente do NativeVoiceProfile semserde. Todos os objetos novos recusam campos desconhecidos/duplicatas; V1 não ganha nenhum campo ou tag. Em85B2, representação ApprovedFilesV2 será própria e os domínios novos usam versão2 e propósitos `native_voice_factory_v2`, `native_voice_eula_v2`, `native_voice_execution_v2`. V1/domínios/Claim/slots/markers conservam bytes e semântica antigos.

APIs internas no mesmo módulo privado de84, sob as mesmas features. Nomes propostos para congelar antes da implementação:

```rust
pub(crate) enum NativeVoiceProfile {
    SimpleVoiceChat,
    PlasmoVoice,
}

pub(crate) fn plan_voice_factory(
    stage: &Path,
    jdk: &Path,
    profile: NativeVoiceProfile,
    accepted: bool,
) -> Result<FactoryPlan>;

pub(crate) fn plan_voice_execution(
    prepared: PreparedStatus,
) -> Result<DedicatedPlan, DedicatedError>;

impl DedicatedRun {
    pub(crate) fn voice_source(&self) -> Option<DedicatedVoiceSource>;
}

impl DedicatedVoiceSource {
    pub(crate) fn observe(&self) -> Result<VoiceRuntimeSample, VoiceRuntimeError>;
}
```

Os nomes legados `PreparedStatus`/`DedicatedPlan` podem continuar internos; a autoridade de perfil vem exclusivamente do bundle opaco. `plan_voice_execution` exigeV2; `plan_status_execution` continua exigindoV1 sem mods. Não adicionar um seletor capaz de relabelar o mesmo prepared. O enum Rust não é desserializável de IPC/JSON e não há um construtor genérico de recursos, receita, processo ou configuração.

Factory/review mostram perfil, versões, baselines, escopo privado e o que será criado. Execução tem propósito próprio, inclui perfil/política/pins/identidades/configuração inicial/argv/env/limites em seu digest e conserva o deadline original do prepared. Uma revisão somente de arquivos/EULA não autoriza Java; planejar voz não renova TTL. O mesmo conjunto concreto de observações aprovadas é comparado no gate final e incorporado ao único claim; não recapturar uma nova identidade de arquivo como se tivesse sido aprovada.

Repetir a revalidação exata de **server.properties e da configuração de voz** em dois pontos de lançamento: antes de remover a autoridade pré-spawn/marcar `MayHaveSpawned`, e depois do checkpoint desse marcador, imediatamente antes do gate final e dispatch83. Cada leitura limitada compara o template compilado correspondente e a mesma Observation do plano original; não atualiza o baseline. I/O fica fora do mutex de lifecycle e os handles curtos são soltos antes de83. Drift antes do marcador recusa spawn e permite apenas o cleanup pré-spawn já autorizado; drift depois do marcador também recusa spawn, mas conserva a condição irreversível e `NeedsRecovery` definida em84. Não converter contador de teste zero em autoridade de restore. Os testes causais alteram bytes e fazem troca byte-idêntica com novo fileID nos dois lados do marcador, exigindo zero spawn real83, edição preservada e a disposição EULA própria de cada fase.

Conservar os tetos84: revisão60s; lifetime300s; readiness180s; stop5s; stdout+stderr4MiB; heap1536MiB; receita e política de retry84 aprovadas. Uma mudança necessária para outro perfil exige revisão explícita deste contrato, não ajuste silencioso para obter PASS. Não acrescentar `preferIPv4Stack`, flags que forcem uma representação de rede ou retries ilimitados.

`DedicatedVoiceSource` contém somente Weak para o estado proprietário, sem proof, `Child`, lock, handle de arquivo, closure de cleanup ou `Arc` de recursos. `Some` identifica um perfil suportado, não voz pronta. Antes de `Ready` de jogo, observe retorna `NotReady`; depois do selo retorna `Closed`; concorrência retorna `Busy` sem fila. A mesma reserva84 é compartilhada com a fonteTCP/readiness. Não acrescentar um segundo slot por protocolo.

`VoiceRuntimeSample` é dado redigido e sem autoridade: perfil/versão fixa, forma de binding observada e anúncio `DefaultUnchanged`. Pela revisão restritiva85A abaixo, somente `Ipv4Literal` pode ser publicado; `Ipv4Mapped` continua não qualificado e recusado. Não inclui PID/handle, caminho local, configuração integral, `server_id`, segredo, token, credencial, hash privado ou capability reapresentável. Não é receipt, permit, prova serializada, ID de ingresso ou indicação de áudio. Erros fechados distinguem `Closed`, `Busy`, `NotReady`, configuração ausente/incompatível/alterada, limite e falha de observação de origem; não exportam linhas da tabela OS ou valores desconhecidos do arquivo.

## 6. Política UDP85 explícita, sem alargar82

Child novo de discovery, enum/export/API e método/variante de consulta83 ficam sob Windows + `connect-lab` + `connect-voice-lab`; fixtures85A usam as mesmas condições. O build só `connect-lab` conserva82/TCP/UDP83 existentes, sem esses nomes novos. Essa precisão foi revista por root/QA antes do código; não acrescenta dependência ou feature.

Novo subcheckpoint interno, distinto de `require_owned_udp_binding`82 e de sua API estrita. Assinatura proposta em discovery, consumida somente pelo observer83:

```rust
pub(crate) fn require_owned_udp_loopback_binding(
    proof: &OwnedChildProof,
    endpoint: SocketAddrV4,
) -> Result<UdpLoopbackRepresentation, DiscoveryError>;

impl OwnedRunObserver {
    pub(crate) fn observe_udp_loopback_binding(
        &self,
        endpoint: SocketAddrV4,
    ) -> Result<UdpLoopbackRepresentation, OwnedRunError>;
}
```

Entrada aceita somente IPv4 literal `127.0.0.1` com porta não zero. O caller85 usa exclusivamente43002 ou43003 derivada do perfil. Não aceita endereço/string/PID de interface, DNS, wildcard, outra família, outro loopback, porta0 ou endpoint selecionado de uma linha encontrada.82 conserva sua política atual, inclusive recusa de mapped, e seus testes não mudam de expectativa.

O novo helper consulta AF_INET e AF_INET6 por `GetExtendedUdpTable` OWNER_PID, em ordem fixa. Antes do I/O valida a entrada e a mesma prova original; depois das consultas/decodificação revalida essa prova, inclusive quando o resultado seria ausência, ambiguidade, erro Win32 ou limite. Se a prova mudou, não devolver uma amostra antiga. Sem abrir processo por PID, adotar replacement ou fazer fallback por PowerShell/Node. A reserva RAII83 é feita antes do I/O e usa o mesmo slot das outras consultas; a decisão final ainda passa pelo gate83.

Cada tabela deve ser integralmente validada antes de procurar/aceitar uma linha: buffer alinhado DWORD, estruturas SDK/offset_of, aritmética checked, comprimento retornado real e não a capacidade arredondada, contagem inteira, nenhuma linha incompleta após um prefixo favorável. Reutilizar apenas helpers privados que satisfaçam esses requisitos. Manter pico de buffer até1MiB, soma dos bytes retornados válidos das duas famílias até1MiB e soma de linhas até16384; liberar o primeiro buffer antes de alocar o segundo, guardando somente contadores/dados candidatos redigidos. No máximo duas tentativas de preenchimento por família. Falha de uma família recusa toda a observação; não ignorá-la para aceitar a outra.

Unicidade conservadora desta política: contar **todas as linhas da porta solicitada**, nas duas tabelas validadas, independentemente de endereço ou PID. Classificar somente se o total for um e essa única linha tiver PID original não zero e uma destas formas exatas:

- AF_INET com octetos `127.0.0.1` e a porta solicitada;
- AF_INET6 com os16 octetos exatos de `::ffff:127.0.0.1`, a porta solicitada e scope0. A requisição não tem flowinfo; qualquer representação interna criada usa flowinfo0. A tabela UDP6 não fornece um campo flowinfo cuja ausência possa ser apresentada como verificação do socket.

**Revisão restritiva85A, antes da correção de código/oráculos:** a classificação estrutural acima permanece, mas a API real aceita somente `Ipv4Literal`. Depois da segunda rechecagem da prova original, candidato `Ipv4Mapped` retorna `DiscoveryError::Unavailable`, inclusive fora de testes; não há knob, fallback, providerCIM, normalização ou caminho permissivo por `cfg(test)`. O observer83 herda essa recusa e conserva o mesmo slot/gate. Um teste negativo com Child/proof original vivo e tabela mapped no seam privado precisa atravessar essa fronteira real; testa a recusa, não qualifica runtime mapped.

A mudança decorre do diagnóstico do **mesmo Child vivo**, com socket AF_INET e socket AF_INET6/V6ONLY0 vinculado explicitamente a mapped. Neste Windows, `Get-NetUDPEndpoint`/CIM apresentou o segundo como mapped, mas `GetExtendedUdpTable` apresentou ambos como linhas AF_INET literais, antes e depois da consultaCIM. A prova original permaneceu válida; stop normal, reap, readers e rebind foram concluídos. Evidência privada: `native-voice-85a-backend-diagnostic-9fae9cf4-9988-439e-b21a-840003db8eec`, diagnóstico1 PASS/0 falhas. A diferença entre providers explica a expectativa incorreta dos primeiros focais; não demonstra que linhas mapped sejam impossíveis em outro ambiente. A documentação distingue a família solicitada da tabela e o comportamento dual-stack: [GetExtendedUdpTable](https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-getextendedudptable), [sockets dual-stack](https://learn.microsoft.com/en-us/windows/win32/winsock/dual-stack-sockets).

Os testes reais passam a exigir a forma literal efetivamente observada, com nomes que distinguem construção dual-stack de linha OS. Nenhum teste aceita qualquer das duas formas ou pula por ausência de mapped. O requisito original de qualificar mapped permanece **BLOQUEADO / NÃO VERIFICADO**, e não é contado como PASS.85B/C podem avançar independentemente; futuros85D/E só podem aceitar linha literal realmente observada. Se Java produzir linha mapped, a observação falha e o processo é recolhido normalmente, sem alteração de flags/parser/política durante o ensaio.82 continua recusando entrada IPv6 mapped; entrada IPv4 lógica pode passar quando a linha OS é realmente AF_INET literal.

Dois registros iguais, V4+mapped simultâneos, linha estrangeira, wildcard, outro endereço/escopo ou PID0 recusam. A regra pode recusar um binding próprio quando outro endereço usa a mesma porta; esse resultado conservador é intencional. Não deduplicar linhas nem converter IPv6 emV4 antes da contagem. Converter porta e scope de ordem de rede; testar scope não simétrico mesmo quando o endpoint aceito exige zero. [Layout e limites da linha UDP6](https://learn.microsoft.com/en-us/windows/win32/api/udpmib/ns-udpmib-mib_udp6row_owner_pid).

Essa contagem não prova exclusividade da porta: as tabelas são amostras sequenciais, podem mudar entre consultas e não identificam o socket físico. Binding pode ser somente para envio; não prova recepção, autenticação, intenção de um mod ou áudio. Release/rebind no mesmo processo pode passar em outra observação; replacement por outro processo não herda o proof. Não prometer prazo máximo da syscall Win32 nem cancelamento instantâneo; trabalho registrado precisa realmente terminar antes da completion.

## 7. Leitura da configuração durante execução

Só os dois caminhos compilados da seção3. Não há API para selecionar arquivo, diretório, nome de mod ou caminho retornado pelo próprio arquivo. Nova leitura privada não chama `inspect_announcement`, não obtém/exibe `VoiceMutationGuard::inner`, não cria mutação por callback, não inicializa banco/Store e não muda Recovery/CAS/Doctor.

Limites máximos por observação:256KiB por amostra de configuração,4096 linhas e os limites sintáticos já usados pelos parsers da voz; no Plasmo,512 chaves,128 tabelas,8 níveis e2048 separadores de array, sem elevar tetos para aceitar o writer. Não emitir conteúdo desconhecido em erro/log. UTF-8 inválido, controles proibidos, ambiguidades, duplicatas de chave relevante, sintaxe não suportada, tamanho excessivo ou leitura instável recusam explicitamente.

SVC: resolver propriedades com as mesmas regras conservadoras de whitespace/escapes/linhas do perfil fixado; exigir `port=43002`, `bind_address=127.0.0.1` e `voice_host` vazio, cada um sem ambiguidade. Não aceitar `-1`,0, wildcard, bind vazio, fallback da portaMinecraft, host externo ou anúncio35554. O parser59 de `voice_host` sozinho não satisfaz essa validação. Chaves adicionais legítimas geradas pelo mod podem ser ignoradas após a validação limitada, sem vazamento de seus valores.

Plasmo: parser TOML limitado, exigir `[host]` com ip literal `127.0.0.1`, port inteiro43003, flags fixas false e ausência de `[host.public]`, inclusive tabela vazia/alternativa aninhada. `43003.0`, herança de porta0, endereço externo ou ambiguidade recusam. Não interpretar nem exportar `server_id` ou credenciais como prova. O caminho é `config/plasmovoice/server/config.toml`; o nome histórico `server.toml` não é alternativa.

Arquivo mutável exige leitor diferente dos pins de artefato. `recovery::secure_open` retém compartilhamento somente de leitura no Windows; `shared_file` compartilha leitura/escrita, mas não delete/rename. Reutilizá-los por toda a consulta ou por toda a execução pode bloquear a reescrita legítima do mod. Criar helper privado de leitura com acesso somente READ, compartilhamento READ|WRITE|DELETE e abertura sem seguir reparse point; verificar tipo/links/identidade do arquivo e pais confinados. Não enfraquecer os helpers Recovery existentes.

Cada leitura é limitada, confere metadata/identidade antes/depois no mesmo handle e solta o handle prontamente. Pais da cópia e dos dois caminhos fixos ficam verificados/fixados enquanto o job os usa; nenhuma referência de source/snapshot retém esses pins. Fazer duas amostras que coincidam em fileID, tamanho e hash, além de semântica, ao redor das consultas de origem. Reescrita entre as duas retorna `Changed`; não esperar o writer estabilizar indefinidamente nem tomar lock exclusivo sobre a configuração.

O mod pode reformatar, acrescentar defaults e trocar o arquivo durante o boot. A dispensa de igualdade ao baseline pertence somente à leitura em execução depois do spawn e à reabertura/recuperação EULA; não se aplica aos gates de lançamento. Antes de83, são obrigatórios template compilado e mesma Observation, inclusive antes/depois de `MayHaveSpawned`, como na seção5. Depois do spawn, uma amostra com outra identidade/hash pode ser aceita se as duas leituras daquela consulta forem estáveis e a semântica fechada continuar válida. Reopen/restore EULA não reexige os bytes/fileID iniciais do arquivo mutável e não o reescreve. Isso não atribui a edição ao mod, não prova continuidade entre amostras e não reativa um serviço que81 tenha desabilitado.

## 8. Ownership, gates e sequência da observação

O worker84 permanece o único dono da transação aplicada, `OwnedRun` e completion83 original. Nada em85 recebe `finish(report)`, um bool de reap ou completion fornecida por caller. Normal request_stop continua sendo chamada síncrona do controle83, separada do cancelamento de STATUS; Drop/Cancel/Deadline e failure latch mantêm suas causas. Config/UDP ausente ou instável em uma observação de voz retorna erro somente dessa observação, preservando STATUS e jogo/TCP. Perda real de processo segue o lifecycle existente.

Sequência proposta de uma chamada:

1. Upgrade Weak e reserva de um job84 sob o gate comum; validar perfilV2, `Ready`, prazo e ausência de selo. Registrar trabalho antes de soltar o gate ou deslocá-lo para execução bloqueante. Chamador assíncrono futuro precisa fazer essa reserva antes de dispatch; cancelar uma future não conta como término.
2. Obter contexto privado imutável da mesma execução: perfil, identidade lab/bundle/cópia, manifest/pins de artefato e proveniência. Revalidar os registros relevantes sem inventariar mundo/logs/arquivos mutáveis nem abrir Recovery para escrita. Não comparar a árvore de jogo em execução ao fingerprint inteiro de antes do boot.
3. Observar TCP43001 pelo mesmo observer83; ler configuraçãoA; observar UDP pela política explícita85; ler configuraçãoB e comparar ambas; observar TCP43001 novamente. Cada consulta83 usa seu único slot e libera seu proof job ao terminar. Ausência de slot recusa, não enfileira nem cria segundo observador proprietário.
4. Revalidar contexto imutável e decidir sob o gate84 original: mesma execução/perfil, prazo original, ausência de selo e configuração estável. Se stop/cancel/expiry venceu, retornar `Closed`, mesmo após resultado OS/config favorável. Nenhum mutex de lifecycle atravessa I/O, await ou join.
5. Soltar bytes/handles/pins/contexto de consulta antes de decrementar o contador RAII e notificar cleanup. Apenas depois devolver o sample redigido. O sample não prolonga processo nem guarda autoridade futura.

Stop sela novas reservas imediatamente e sinaliza o83 original sem esperar a leitura bloqueante.83 pode encerrar/reap/readers antes de a consulta84 acabar;84 espera também o job84 realmente terminar antes de restaurar EULA, liberar recursos finais e publicar completion. Waiter perdido acompanha a mesma conclusão. Completion mantém somente resultado/watch, sem `Arc` de Child/proof/pins/readers ou closure de cleanup. Não publicar complete por timeout do waiter, cancelamento da query ou contagem que caiu antes do último owner de recurso.

Nenhuma sequência de snapshots elimina corrida externa depois do último gate, identifica qual socket físico responderia ao próximo pacote ou demonstra que o JAR selecionado abriu aquela porta. O máximo de evidência é: esse processo original estava vivo, possuía a linha permitida nas amostras OS e a configuração observada concordava com o perfil fixado durante a consulta. Não chamar isso `VoiceReady`.

## 9. Checkpoints e provas causais locais

Todas as fontes ficam congeladas antes do Cargo coordenado pelo root. Fixtures são filhos próprios ocultos, stdout/stderr limitados, RAII instalado desde spawn, protocolo de controle fechado e fallback somente para o próprio filho. Não executar Java para substituir prova ausente de ownership, nem usar provider falso de STATUS/UDP para marcar runtime real.

| Checkpoint | Prova exigida |
| --- | --- |
|85A — API UDP separada e proprietário83 | Decoder com primeira linha favorável e última truncada, returned_bytes menor que backing completo, overflow/count/limites, scope não simétrico, duas famílias e múltiplas linhas da mesma porta; erro de uma família não é ignorado; prova original rechecado após erro/sucesso. Filho próprio com bindV4 e construção dual-stack mapped, exigindo linha literal OS realmente observada conforme revisão restritiva da seção6; gate runtime mapped original permanece bloqueado e API o recusa. Foreign/parent/wildcard/duas linhas/porta ocupada/escopo inválido; release/rebind e replacement; integração no mesmo QueryJob/gate/Weak83, slot único compartilhadoTCP/UDP; stop durante syscall não publica amostra tardia.82 literal continua passando sem mudar sua política. Esse checkpoint não depende de V2/factory e não executa Java. |
|85B — ManifestV2 e factory | Golden de bytesV1 persistidos, manifest/claim hash/digest/review e recovery de cada prefixo84; dispatch real recusa duplicatas de schema/chaves e schemas/campos desconhecidos sem Value→reencode que apague evidência; V2 com contagens/pins/IDs/perfil fechados; duas factories reais `diagnostic_lab`; outro mod ausente; criação exata dos baselines; NoInit; rejeição de rename/root transferido, links, arquivos extras e substituição byte-idêntica após aprovação; TTL A→execução não renovado; zero mutação/spawn negativo ancorado em execução positiva real da mesma API. |
|85C — leitura/lifecycle sem Java | Fixture do binário cria configuração e sockets reais; reescreve via rename enquanto a consulta está retida para provar que o reader não bloqueia o writer; divergênciaA/B, novo fileID, arquivo ausente, links, oversize e semântica divergente recusam. Troca entre consultas estáveis é aceita somente nos termos documentados. Gates antes da reserva, depois de resultadoOS positivo e antes da decisão final reproduzem stop/cancel/deadline, `Busy`, lost waiter e Drop. TCP/STATUS com bytes continua após erro somente de voz. Reap/readers/portas liberadas podem ocorrer com job84 retido; EULA/locks/completion só terminam depois desse job. |
|85D — SVC nativo real | Nova cópia e plano de execuçãoSVC, recursos locais rechecados, receita explícita, Java83 real, STATUS767/pong, leitura da configuração produzida durante a execução e UDP próprio com representação registrada. Encerrar pela completion original, conferir EULA restaurada, claim consumido, origem/recursos intactos, ausência de filhos/readers próprios e rebind das portas controladas. Nenhuma GUI/cliente/áudio. |
|85E — Plasmo nativo real | Repetir85D em nova cópia com Plasmo + Fabric API, SVC ausente, configuração/caminho/pins próprios. Registrar a representação real e limites separadamente; sucessoSVC não é provaPlasmo. |

Os testes mapped reais não podem virar PASS por ausência de IPv6/mapped no ambiente nem por flags novas no Java. O diagnóstico registrou esse bloqueio, e a revisão da seção6 restringe a API ao ramo literal realmente qualificado antes de qualquer Java; o ramo mapped não pode ser usado em Java enquanto bloqueado. Os decoders sintéticos sozinhos não qualificam o runtime. A documentação Microsoft descreve linhas UDP como sockets vinculados, não como prova de intenção de receber; o nome do teste deve refletir isso.

No Java real, reaproveitar o opt-in/receita supervisionada84, sempre gerando factory/review/cópia novos para cada perfil. Nenhum download novo ou leitura de credencial. Se configuração, bind, política, limites ou artefatos diferirem, falhar, recolher o processo e restaurar EULA quando a autoridade original permitir; registrar erro fechado. Não ajustar parser, porta, heap, timeout ou retry no meio do ensaio sem nova revisão técnica e oráculo causal.

Fixtures podem usar seams locais `cfg(test)` de fase/relógio reduzido/falha com RAII de release; não introduzir providers/closures livres de produção, PID externo, clock global ou aprovação de arquivo falso. Não atribuir zero-spawn a contador desconectado de `Command::spawn`. Não atribuir prova de Java concorrente a um probe que o caminho normal nunca chama. A separação entre criação privada e guard de recuperação pode ser documentada por leitura de código enquanto não houver um caso real próprio adicional autorizado.

## 10. Reconciliação com backlog e fronteiras ainda pendentes

85 é continuação parcial de **AUR-R4-006** SVC e **AUR-R4-007** Plasmo, com lifecycle de005 e robustez008; transporte003 continua dependência do futuro vínculo. Não inventar IDs ou matriz85. Os candidatos1.20.1 do início do backlog são históricos; seus próprios registros posteriores já selecionaram1.21.1/Fabric para os perfis locais. Esta proposta não converte linhas de caminhos externos em homologadas.

| IDs existentes | O que85 pode comprovar | O que permanece pendente |
| --- | --- | --- |
|T-VOICE-01 | Inventário fixado e configuração limitada observada no dedicado nativo, separadamente por mod. | Outros loaders/versões/hospedagens e contrato de autenticação funcional. |
|T-VOICE-02 | Bind do processo original e configuração local de anúncio padrão/ausente, como amostras distintas. | Anúncio realmente recebido pelo cliente, ponte por participante e endereçamento fim a fim. |
|T-VOICE-03/05/06 | Nenhuma homologação por85. | Dois clientes reais, áudio sem gravação, modo misto e distribuição real de datagramas/overhead. |
|T-VOICE-04/07 | Negativos locais de artefato/config/binding/porta e preservação do jogo após erro de observação. | Credencial do mod, pontes de duas instâncias, reconexão/sessão antiga e autenticação do retorno. |
|T-CONFIG-01..04 | Criação compilada em cópia nova e preservação negativa; compatibilidade dos journals EULA84. | Não substitui as provas de aplicar/restaurar configurações pessoais58/67/68. |
|T-LIFE-01, T-MIX-03 | Ownership/cleanup nativo e erro somente de voz sem encerrar jogo. | Suspensão/rede T-LIFE-02 e revogação real de uma sessão de voz transportada. |
|T-MAN-01, T-REG-01/02 | ManifestV1 preservado, perfisV2 fechados, regressões apropriadas. | Distribuição, release e promessas públicas de compatibilidade. |

Depois de85, ainda será necessário um contrato de vínculo entre fonte nativa, revisão de anúncio, serviço e participantes.64 aceita UDP manual e não pode ganhar autoridade de voz por reutilizar uma porta observada.67/68 comprovam aplicação/restore de configuração; não comprovam o processo, socket ou transporte que a consome. Recibo durável, digest, nome de arquivo, porta fixa e log do mod não substituem prova do proprietário.

O futuro Host jogador precisa de participante local explícito, ocupando uma das oito vagas, ponte35554 e conflitos/retorno testados;85 não cria isso automaticamente. Monitor de mudança e vínculo por lease/serviço deverão retirar somente UDP por81, mantendo game/TCP e os demais participantes. A retirada81 é permanente para aquela revisão/serviço, sem renovar grants/clocks/quotas, e espera trabalho/owners reais. Nova amostra85 favorável não reativa o serviço.

Sem protocolo novo, o Guest não recebe notificação instantânea de encerramento da associação; datagrama já enfileirado emQUIC pode chegar depois do selo. O futuro vínculo não pode prometer mute remoto imediato ou zero payload já enviado. Limites81 de V1/V2, fragmentação/cotas, admissões e cleanup permanecem. A observação85 por si só não autoriza encaminhar UDP, associar o socket a um mod específico ou falar em voz autenticada.

Assistente desktop, sessões independentes, alpha.14, nove pares Core/Skins, Recovery/CAS/Doctor e gates R0/R1/R1.5/R2/R2.5/R3/R5 ficam preservados. Não há Companion/HUD/Assistente no jogo. Nenhum resultado local altera relay público, NAT, firewall, redes externas ou capacidade de gastos.

## 11. Aceite, rollback e estado permitido

Aceite85 exige compatibilidade demonstrada dos journals84, factory real de cada perfil, políticaUDP explícita e testada com owner original, leitor que não bloqueia o writer, preservação do lifecycle84/83 e ensaios nativos reais separados de SVC/Plasmo. Compilar, ler config, ter um JAR, enxergar uma porta ou repetir um resultado Node não satisfaz esses critérios. Documentar falhas/retries usados e não somar rodadas sobrepostas como testes novos.

Estado máximo permitido: **PROTÓTIPO de dedicado nativo com configuração e binding UDP observados**, por perfil que realmente passou. Serviço vinculado ao Host, ponte local, autenticação do mod, voz audível, GUI e dois PCs continuam **NÃO IMPLEMENTADO / PRECISA DE TESTE**, conforme o respectivo item. Sem PASS e commit84C este contrato não libera implementação85; sem aprovação integral root/QA deste texto não se registram novas fontes.

Rollback operacional: solicitar stop e aguardar a mesma completion original com queries/readers/reap e EULA; divergência conserva `NeedsRecovery` e evidências para o guard/review fechado, sem reconstruir slots nem adotar PID sobrevivente. Não restaurar automaticamente configuração de voz produzida pelo mod. Cópia usada permanece consumida, inclusive após restore; não remover claim para tentar novamente.

Rollback de código é local e revisado, preserva85V2 pendente até recuperação por código que o entenda e não altera84V1. Feature permanece off; não há publicação, instalador, alteração de configuração pessoal ou migração global. Implementação/testes/revisão locais usam as autorizações existentes, sem solicitar nova confirmação humana; futuras provas de áudio/participantes externos são etapas próprias, não requisitos humanos inventados para concluir este recorte local.

## Registro da preparação

Na elaboração do texto foram feitas somente auditoria de fontes/contratos e leitura limitada de recursos locais. Consultadas documentação oficial SVC/Plasmo e estrutura WindowsUDP6. Nenhum código85, Cargo, Java, download, alteração canônica ou operaçãoGit foi executado pelo autor da proposta. Os checkpoints acima eram critérios futuros; revisão integral root/QA e hash estão registrados no cabeçalho. O gate PASS/revisão/commit84C foi cumprido antes de liberar85A. Resultados85 permanecem ausentes até sua execução, separados dos testes84.

## Registro da execução85A

Estado85A: **PROTÓTIPO**, somente leitura UDP e integração83, sem Java/mod/voz novos. Features Windows/connect-lab/connect-voice-lab permanecem default-off;82 e produtor84C preservados. Arquivos: `connect_discovery/mod.rs`, `connect_discovery/udp.rs`, novo `connect_discovery/udp/loopback.rs` e testes filhos; `connect_owned_run.rs`, registro/fixture de testes e novos `tests/udp_loopback_{fixture,queries}.rs`. Canonização deste domínio em AGENTS/SPEC/ADR/backlog/matriz/task não promove os gates de produto.

Primeiros focais: discovery14 PASS/3 falhas/1 ignorado e owner36 PASS/3 falhas/1 ignorado, todos os erros positivos por expectativaMapped recebendoLiteral. Diagnóstico SAMEChild1 PASS confirmou as representações diferentesCIM/IPHelper e cleanup normal. A revisão restritiva acima foi aprovada antes de mudar código/oráculos; mapped permanece bloqueado. Comandos, tempos e logs estão no [task](task-aur-r4-connect.md).

Focais corrigidos realmente executados com `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline --locked --jobs 2 --all-features --lib <filtro> -- --test-threads=1`: filtro `engine::connect_discovery::udp::loopback::` **18 PASS/0 falhas/2 ignorados**,0,61s/compilação36,45s; filtro `engine::connect_owned_run` **39 PASS/0 falhas/1 ignorado**,8,18s/compilação1,38s. Fmt check PASS. Os dois ignores discovery são helper próprio e diagnóstico coordenado, não skip de qualificação mapped; o diagnóstico já teve execução exata separada. Teste de scope inválido é decoder sintético; não é socket real com scope inválido. O novo negativo atravessa a API real com Child/proof original, tabela mapped sintética e exige Unavailable; morte após classificação prevalece como ProcessChanged. Positivos reais incluem bytes1264, foreign/parent, mesma porta em outro endereço, release/rebind e replacement.83 compartilha Busy/slot entre as três APIs e sela resultados tardios, com reap/readers antes da liberação da query retida.

Regressão all-features/all-targets **633 PASS/0 falhas/70 ignorados**, oito targets,693,07s somados de testes/compilação1m14s; biblioteca524 PASS/68 ignorados. Fontes congeladas durante cada Cargo. Primeiro Clippy recusou apenas `unnecessary_unwrap` no seam de teste; troca equivalente para if-let não alterou produção/oráculos. Teste exato afetado reexecutado **1 PASS/0 falhas**,0,12s/compilação12,33s. Full633 é anterior a essa troca sintática, sem alegar full posterior. Clippy final all/default/connect **PASS13,38s/5,22s/9,20s**, fmt PASS, todos exit0/sem avisos finais.28 pais ativos novos pertencem a85A; não somar os focais/diagnóstico ao full. Detalhes, arquivos, comandos e logs no task. Novo draftB0 não registrado e proposta histórica75 ficaram fora da revisão/commit85A.

Rollback mantém features off, encerra somente filhos próprios pela completion original e preserva evidências; não muda configuração pessoal. Próximo trabalho local autorizado é85B, começando pela preservação byte-exata de manifests/journalsV1 e prefixos reais antigos antes da refatoraçãoV2; o gate mapped não bloqueia essas provas independentes. Nenhuma ação humana é necessária para essa continuidade local.

## Registro da execução85B0

Base85A commitada em `6cded7fade77fb8915e4f4f9c211f83a93fa64c4`. B0 não altera o formatoV1 ou concedeV2. Factory/EULA/Execution agora chamam helpers de preimagem privados compartilhados com os replays; Recipe usa o mesmo corpo/ordem e Resources verificados. ApprovedFiles só ganhou Deserialize em cfg(test), sem alteração de campos/Serialize. TestRoot tem retenção explícita/defaultfalse e fecha pins normalmente. Seis Rust: `connect_dedicated/{factory,eula,producer,tests}.rs`, `connect_dedicated/tests/files.rs` e novo `tests/v1_upgrade_capture.rs`.

Os três opt-ins são dados/testes sem start/Child/Instance/guard retornado por JSON: golden antigo10 arquivos **1 PASS**; captura de13 prefixos e verifier baseline V1→V1 **1 PASS cada**, com identidade/bytes/preimagens e guard/review novos, cleanup somente após todos os oráculos. Depois foram capturados outros dois conjuntos separados de13, cada captura **1 PASS**, nos mesmos paths/IDs que serão consumidos respectivamente depois deB1 eB2. Os antigos Snapshot/Stage continuam NeedsRecovery. Isso testa o harness antes da mudança; as reservas não são prova pós-V2 antecipada e as três rodadas de captura não são testes novos distintos.

Regressões focais: transactions **5 PASS**, factory/EULA normal **1 PASS**, STATUS sintético com Child original/cleanup **1 PASS**. Clippy all/default/connect **PASS14,05s/1,15s/1,31s**. Fmt inicial PASS; limpeza apenas de comentários deixou um LF extra no primeiro fmt final, corrigido e fmt final PASS. Nenhuma mudança comportamental para obter esses resultados. Full633 pertence a85A e não foi reatribuído/reexecutado porB0. Receita usa synthetic=false dentro do binário de teste; isso não é prova de JAR/JDK reais ou Java executado. Nenhum Java/mod/áudio/rede novo, configuração pessoal ou matriz alterada.

Índices privados reservados: B1 `native-status-v1-upgrade-78b4c00a-6f63-4d11-b3dc-06008f5a376c.json`, SHA44817dc5b078730790ee96c6888c229caef9ef967b40baa50a7301f735b38885; B2 `native-status-v1-upgrade-b2573746-aee0-414b-9ef8-1558c38e43c1.json`, SHA2610827cbdba9324c2388d5a2e2d2abddbcb000a4949b3efccf3547f572a0896. Não mover/copiar/relabelar roots ou consumir a reservaB2 no testeB1. Relatório privado CreateNew `native-voice-85b0-results.json`, SHA766a049eb338b87b47545462986722c1635e3b03ca0e1e5769f63912bdd228a8. Comandos/tempos e rollback no task; é evidência sem autoridade de restore.
