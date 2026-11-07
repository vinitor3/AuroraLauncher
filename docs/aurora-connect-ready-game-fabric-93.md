# Connect93 — segunda cópia pronta Fabric

Estado: **PROTÓTIPO**, implementação ligada ao fluxo nativo e frontend, com testes nativos, DOM e probe Java17 executados. Nenhuma homologação de jogo é afirmada. Reutiliza o contrato73 e a mesma revisão, consentimento, cópia diagnóstica, mutation guard, Recovery, Doctor, IPC e gate nativo de spawn.

O único perfil acrescentado é Minecraft1.20.1/Fabric0.16.14/Java17, com Core1.1.5 e Skins0.3.1 já instalados/verificados. O perfil anterior Minecraft1.21.1/Fabric0.16.14/Java21 permanece. Ambos já pertencem à matriz ativa. Os outros sete pares continuam sem autorização nesta ação até implementar e validar seus adaptadores; STATUS91 não concede prontidão ou autoridade de lançamento. Base desta etapa: checkpoint `0bca0ea`, posterior à entrega local90/91/86A1.

O [manifesto oficial 1.20.1](https://piston-meta.mojang.com/v1/packages/72ae93b0934df5541860e3e6b72cf883c74da491/1.20.1.json), obtido via manifesto Mojang e verificado por SHA1, declara Java17 e `is_quick_play_multiplayer`/`--quickPlayMultiplayer`. O [perfil oficial Fabric 0.16.14 para 1.20.1](https://meta.fabricmc.net/v2/versions/loader/1.20.1/0.16.14/profile/json) usa KnotClient, herda1.20.1 e declara `net.fabricmc:fabric-loader:0.16.14`. Projeções e pins dessas fontes estão nas fixtures91. A integração produz exatamente um par QuickPlay a partir da ponte nativa original, sem aceitar destino/versão/argv no pedido do renderer.

Um descritor fechado de perfil é escolhido pelos metadados instalados, conferidos contra os mesmos arquivos retidos na prova. Não é seleção por string livre ou inferência pelo nome da cópia. Versão, loader, Java esperado, arquivos Core/Skins, hashes e argumentos devem concordar; metadados conflitantes falham sem instalar ou reparar. O probe real do Java selecionado precisa corresponder ao major do descritor, com os arquivos do runtime fixados durante a observação. A revisão/digest/Doctor recebem os valores efetivamente verificados, não constantes1.21.1/21.

A capability nova tem schema2 e dois perfis explícitos. O frontend continua lendo schema1 legado como autorização exclusiva para o perfil anterior; uma revisão1.20.1 não pode ser confirmada sob capability1. Ausência/falha da capability conserva consulta/cancelamento do trabalho existente. O request e confirmação mantêm seus campos73, sem versão, loader, porta ou argumentos novos.

Ficam preservadas a identidade offline existente, nonce efêmero IPC por execução, aparência pública e cache compartilhado. Não há autenticação Microsoft, mudança de conta, atualização de mod/Java, instalação automática, lançamento sem consentimento ou promessa de mundo/áudio. A integração social92 possui sua própria prova de grant/conta/continuidade;93 não cria fonte de transporte a partir de `tcpAddress`.

Aceite local: perfis oficiais dos dois alvos, argv QuickPlay exato, recusa de Java errado, metadados/JARs/paths e argumentos conflitantes, hashes Core/Skins de cada alvo, preparo/read-only e revalidação antes do gate, troca de perfil entre revisão/execução, capability1/2 e pares incoerentes no DOM. Reutilizar regressões de cancelamento/conta/perda de IPC; executar probe Java17 real separado se um runtime já estiver disponível. Nenhum teste sintético ou probe Java será chamado de homologação Minecraft. Entrada visual, Core/Skins em mundo e encerramento do cliente continuam **PRECISA DE TESTE** até evidência própria.

## Os outros sete pares

| Pares existentes ainda fechados nesta ação | Requisito concreto antes de abrir o gate |
| --- | --- |
| Forge1.12.2/Java8 | Política de entrada anterior ao QuickPlay comprovada no cliente, runtime Java8 não modular, LaunchWrapper e inspeção de identidade `mcmod.info`/JARs legados. |
| Fabric e Forge1.16.5/Java8 | Política de entrada anterior ao QuickPlay, pinning real do layout Java8 e reaplicação explícita de `VersionLaunchSpec::apply_offline_compatibility`, já usada pelo launch normal para multiplayer offline nessa versão. Forge exige também sua inspeção própria. |
| Fabric e Forge1.19.2/Java17 | Política de entrada anterior ao QuickPlay; os manifests oficiais consultados não declaram a feature. Forge acrescenta validação de argumentos FML/module-path e `mods.toml`/JarJar. |
| Forge1.20.1/Java17 e1.21.1/Java21 | QuickPlay existe na base oficial, mas a prova de mods precisa reconhecer `mods.toml`/JarJar, identidades aninhadas e caminhos de módulos Forge.1.21.1 usa ForgeBootstrap, diferente do BootstrapLauncher de1.20.1. |

O verificador73 de JARs adicionais entende Fabric e seus JARs aninhados; somente trocar o nome do Core/Skins esperado deixaria de verificar identidades Forge aninhadas. A política atual também recusa os module-paths dos perfis Forge oficiais. Esses controles precisam de adaptadores tipados com caminhos confinados e testes negativos, sem remover recusas para fazer um perfil passar. O catálogo91 identifica apenas a versão do STATUS e não substitui essas provas.

## Validação local

### Fontes primárias e limite da evidência

As [notas oficiais do Minecraft 1.20, seção Quick Play](https://feedback.minecraft.net/hc/en-us/articles/16499677456781-Minecraft-Java-Edition-1-20-Trails-Tales) documentam que `--quickPlayMultiplayer` recebe o endereço do servidor e solicita entrada direta. O manifesto exato1.20.1 confirma que essa versão possui a feature. No [MinecraftGameProvider do Fabric Loader 0.16.14](https://github.com/FabricMC/fabric-loader/blob/075dbfa476b1259f1d04a96c2b9afc6bd8f06c4e/minecraft/src/main/java/net/fabricmc/loader/impl/game/minecraft/MinecraftGameProvider.java#L463), o loader invoca o entrypoint do jogo com os argumentos analisados. Isso sustenta o caminho de argumentos esperado; executar o cliente ainda é necessário para comprovar entrada em mundo.

Consulta somente de leitura repetida em07/11/2026: os bytes do manifesto Mojang tiveram SHA1 `72ae93b0934df5541860e3e6b72cf883c74da491` e SHA256 `c966587c030c0c1ccfd60af3188be9a6b9b1aeb8299a0da0666fffd3d995e351`; o JSON Fabric teve SHA256 `9ceea5fa8adad79f5ca5eeb800b164a57a0fd66b05d5ef1c03f2de01c9442064`, igual ao pin91. O tag Fabric0.16.14 resolveu para commit `075dbfa476b1259f1d04a96c2b9afc6bd8f06c4e`; o arquivo de fonte consultado teve SHA256 `b62d0699be324c174123e75da7354da2dfe6d5cf13fafc181d9b1502d497b5c2`. Nenhum desses hashes é apresentado como assinatura de JAR nem prova de execução.

O resolver mantém desabilitadas as features QuickPlay do manifesto durante a resolução comum. O adaptador fechado valida o spec e acrescenta um único par multiplayer; não habilita `has_quick_plays_support`, não preenche `${quickPlayPath}` e não cria destino de log QuickPlay. A fixture1.20.1 usa os argumentos oficiais da base, o perfil Fabric capturado e os Core/Skins embutidos dessa versão; os demais artefatos de jogo e Java da fixture são inertes.

### Testes executados

A primeira rodada nativa93 teve17 PASS,3 FAIL e2 ignorados em17.98s, registrada em `.aurora/connect-validation/resume92-93-connect_ready.log`. Os três casos Fabric1.20.1 pararam em `ConflictingArguments`: a política73 recusava incondicionalmente `-Dorg.lwjgl.system.SharedLibraryExtractPath`, presente nos argumentos oficiais Mojang mesmo quando resolvido para a pasta `natives` da própria cópia. A correção conserva os argumentos oficiais e limita as quatro propriedades de carga/extração nativa (`java.library.path`, `jna.tmpdir`, `org.lwjgl.system.SharedLibraryExtractPath`, `io.netty.native.workdir`) ao caminho exato observado; valor ausente/vazio, destino externo, travessia, lista e sufixo são recusados. O teste1.20.1 verifica os quatro valores; um caso adicional usa os argumentos oficiais1.21.1 e testa as recusas sem alterar argv. A falha inicial é preservada.

Retestes coordenados pelo integrador: `connect_ready`20 PASS/2 ignorados em17.43s; `connect_quick_play`9 PASS em0.92s; `ready_game`15 PASS/1 ignorado em9.97s, incluindo a projeção Fabric1.20.1; `social`20 PASS/2 ignorados em1.47s. Logs `.aurora/connect-validation/resume92-93-{filtro}-fixed.log`. O opt-in Java17 real passou1 teste em4.75s, log `.aurora/connect-validation/resume93-java17-real.log`. Os filtros têm possíveis sobreposições e não são somados como testes únicos. Os ignorados normais incluem fixtures de filho próprio e opt-ins explícitos; somente o probe Java17 citado foi executado separadamente por esta evidência.

`node --test src/features/connect-game-launch/ReadyGameLaunchProfiles.test.mjs src/features/connect-game-launch/ReadyGameLaunch.smoke.mjs` passou32 testes, zero falhas/ignorados,928.9883ms. São29 casos DOM (dois novos para1.20.1 e retirada da autorização schema2) e três contratos novos puros. Uma rodada anterior, antes desses dois casos DOM, executou os mesmos componentes junto de ConnectView e passou240; os totais não são somados. Os testes verificam pedido/confirm idênticos ao73, nenhuma versão/argv/porta no request, nenhum launch automático, Java17 somente em1.20.1 e impossibilidade de confirmar esse perfil com schema1 ou capability ausente.

Foi usado o Java17 existente Temurin17.0.20.1+1, no caminho abaixo. A inspeção de arquivos confirmou o layout modular esperado e o opt-in `ready_fabric1201_actual_java17_probe_under_retained_runtime_pins` passou na rodada coordenada acima. Checks gerais/build integrado permanecem no relatório do integrador. Nenhum Minecraft foi iniciado nesta etapa.

### Procedimento limitado para o Java17 real

O integrador executa o opt-in isoladamente, depois de encerrar as outras rodadas Cargo/fixtures. O caminho observado é `C:/Program Files/Eclipse Adoptium/jdk-17.0.20.101-hotspot/bin/java.exe`. O teste constrói somente a cópia diagnóstica de fixture em diretório temporário próprio, substitui seu caminho Java pelo executável já instalado e mantém os handles de leitura da prova durante `-version`. Não instala Java, não executa os JARs inertes, não abre Minecraft ou servidor e não altera uma instância pessoal.

```powershell
$env:AURORA_CONNECT_READY_JAVA17 = 'C:/Program Files/Eclipse Adoptium/jdk-17.0.20.101-hotspot/bin/java.exe'
try {
    cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --all-features engine::connect_ready_runtime::tests::fabric1201::ready_fabric1201_actual_java17_probe_under_retained_runtime_pins -- --ignored --exact --test-threads=1
} finally {
    Remove-Item Env:AURORA_CONNECT_READY_JAVA17
}
```

O helper nativo chama diretamente o executável, sem shell, remove `JDK_JAVA_OPTIONS`, `JAVA_TOOL_OPTIONS`, `_JAVA_OPTIONS` e `CLASSPATH`, retém stdout/stderr somente em memória, limita a saída combinada a16KiB e a observação a5s. Em erro/cancelamento/prazo, encerra e aguarda somente seu filho; a espera de reap do sistema operacional pode exceder o prazo de observação. O teste exige major17 e revalida os pins antes de soltá-los. Registrar resultado e duração reais; mesmo PASS comprova apenas o probe e a retenção desses arquivos, sem homologar o cliente.

### Revisão da composição com92

A inspeção de código confirmou que o perfil93 entra no digest e é comparado novamente com fingerprint e hashes antes do lançamento. O gate92 continua cobrindo o `spawn` e o registro do filho sob o mesmo Connect/Attempt, ponte original, grant/convite social, continuidade e epoch de conta. Não foi identificada autoridade derivada de campos do renderer nessa composição. A fixture social92 declara Fabric1.21.1; a projeção nativa93 de Fabric1.20.1 possui caso separado. Esses testes não devem ser relatados como uma execução social real do cliente1.20.1.
