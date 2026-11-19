# Java e engine — configurações de inicialização

**IMPLEMENTADO MAS NÃO HOMOLOGADO**. O launcher resolve a RAM ao preparar cada inicialização.

## Memória padrão para os modpacks

Em **Java & engine**, escolha um valor em MB ou use os atalhos em GB e clique em **Salvar padrão**. Instâncias novas e existentes que usam o padrão recebem esse limite na próxima execução. O valor inicial é 4096 MB. Marque **Incluir modpacks com memória personalizada** e clique em **Aplicar a todos** para que as configurações anteriores de RAM também passem a seguir o padrão. O coletor e os argumentos JVM de cada instância são preservados.

Para mudar uma única instância, abra **Instâncias → Editar → Memória**. Escolha **Manual**, **Automático** ou **Padrão do launcher** e salve. O coletor de memória e os argumentos personalizados ficam em **Opções avançadas da JVM**.

A preferência compartilhada fica em `launcher-memory.json` no diretório de dados do aplicativo. Aplicar a todos troca a revisão de memória nesse arquivo em uma única gravação atômica. O launcher lê essa revisão ao preparar o jogo e as cópias do Connect. Se outra janela aplicar a memória a todos durante uma edição individual, o editor pede a releitura dos valores antes de salvar. Alterações após a revisão de uma cópia pronta exigem revisar o preparo. A memória de jogos já em execução é mantida pelo processo Java.

## O que já existia

Inventário/seleção/instalação de Java, catálogo Temurin, verificação de hashes e detecção de requisitos Fabric, inclusive JARs internos, já estavam implementados. Não foram substituídos. A auditoria do DOCX `Aurora_Status_e_Reorganizacao_2026-08-29.docx` identificou JVM-01/OPEN-07: a fórmula de RAM existia, mas o launch fixava 4096 MB. O documento pede inventariar RAM/mods e aplicar flags por Java, sem especificar um pacote de flags ou comandos antes/depois do jogo.

## Configuração por instância

O arquivo local `.aurora/launch-settings.json` pertence à instância. Não contém caminho Java, conta, credencial ou comando de shell. Uma instância sem arquivo segue o padrão do launcher. Configurações salvas nas versões anteriores conservam os valores individuais até a aplicação em lote. Campos antigos opcionais ausentes recebem os padrões da versão original. Versão desconhecida, campos desconhecidos, conteúdo inválido e arquivo acima de 32 KiB são recusados; não são silenciosamente usados como defaults. O painel permite carregar os padrões nos campos e só substitui o arquivo após **Salvar configurações**.

- **Manual:** máximo entre 512 e 262144 MB; inicial opcional entre 512 MB e o máximo. Se a RAM física for conhecida, um máximo maior que ela é recusado.
- **Automático:** conta JARs regulares habilitados diretamente em `mods`, sem contar subpastas, links ou `.disabled`, com enumeração limitada a 10000 entradas. Usa a fórmula existente `min(max(mods × 48, 4096), RAM_total − 2048)`, com saturação e teto de 262144 MB. Recalcula no launch, depois da instalação dos módulos internos. Resultado menor que 512 MB ou RAM desconhecida bloqueia esse modo, sem emitir `-Xmx0M`; use manual.
- **Coletor padrão:** preserva a escolha da JVM ou do perfil de versão/loader.
- **Coletor recomendado:** escolhe G1 no Java 17 ou superior; abaixo de 17 preserva o padrão. É uma política conservadora explícita, sem promessa de FPS ou pausas melhores. Os documentos originais não especificavam flags exatas. A [orientação oficial do G1](https://docs.oracle.com/en/java/javase/17/gctuning/garbage-first-garbage-collector-tuning.html) recomenda começar pelos padrões e pelo tamanho do heap.

O inventário físico usa [GlobalMemoryStatusEx](https://learn.microsoft.com/en-us/windows/win32/api/sysinfoapi/nf-sysinfoapi-globalmemorystatusex) no Windows. Outros sistemas retornam RAM desconhecida e mantêm o modo manual disponível. RAM total não é RAM livre; a reserva de 2 GiB é heurística e não coordena a memória de jogos simultâneos.

## Argumentos aceitos

Cada linha vira um elemento de `argv`, sem parser de shell, aspas ou substituições. Limite: 64 argumentos (63 personalizados no modo GC recomendado, reservando um para G1), 256 caracteres ASCII por argumento, sem espaços ou controles. A lista permitida é fechada:

| Opção | Limite |
| --- | --- |
| `-XX:+UseG1GC`, `-XX:+UseParallelGC`, `-XX:+UseSerialGC`, `-XX:+UseZGC` | Um coletor; modo de coletor padrão |
| `-XX:MaxGCPauseMillis=N` | 1–10000 |
| `-XX:G1ReservePercent=N` | 0–50 |
| `-XX:InitiatingHeapOccupancyPercent=N` | 1–100 |
| `-XX:ParallelGCThreads=N`, `-XX:ConcGCThreads=N` | 1–256 |
| `-XX:+/-ParallelRefProcEnabled`, `-XX:+/-UseStringDeduplication`, `-XX:+/-AlwaysPreTouch` | Sintaxe literal `+` ou `-` |
| `-XssNk`, `-XssNm` | 256–16384 KiB ou 1–16 MiB; sufixo também aceita maiúscula |
| `-ea`, `-da` | Assertions |
| `-Dfile.encoding=UTF-8` | Valor literal |
| `-Duser.language=pt`, `-Duser.country=BR` | 2–3 letras minúsculas ou 2 maiúsculas |

O Java selecionado ainda pode recusar uma opção incompatível com sua versão ou distribuição. Não se detecta suporte a cada flag nem se faz benchmark automático. Agentes, `@argfile`, classpath, main class, argumentos de jogo, `-Daurora.*`, propriedades arbitrárias, hooks e opções que executem comandos são recusados. `-Xmx`/`-Xms` são definidos somente pelos campos de memória.

O engine remove aliases de heap vindos do perfil e emite um único máximo/inicial. Quando existe coletor explícito nas configurações efetivas, ele substitui a seleção do perfil, evitando dois coletores. O processo do jogo não herda `JAVA_TOOL_OPTIONS`, `JDK_JAVA_OPTIONS`, `_JAVA_OPTIONS` ou `CLASSPATH`; o ambiente do launcher e do sistema não é alterado. O preview redige access token e nonce IPC. Nenhuma linha de comando é registrada.

## Persistência e integração

Os comandos `read_launch_settings` e `save_launch_settings` validam IDs e leem/gravam apenas o caminho fixo da instância. Salvar exige exclusão mútua e jogo fechado, verifica JVM sobrevivente no Windows e bloqueia uma tentativa `preparing`/`running`. Links/reparse points nos componentes são recusados. A publicação usa staging único, `sync_all` e substituição no mesmo volume ([MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw) no Windows). Uma falha de publicação preserva a versão anterior e remove apenas o staging desta tentativa.

Doctor registra a memória efetiva da execução; a análise sem uma execução solicitada resolve a configuração atual. Preferências inválidas não desativam o Doctor: ele informa essa limitação e usa a memória da tentativa anterior ou 4096 MB como referência explicitamente identificada, sem usar esse fallback no lançamento. Java é verificado também no launch vanilla, preservando os requisitos existentes por Minecraft/mods. Os locks, a recuperação/CAS e o laboratório alpha.14 continuam no mesmo caminho de preparação. Configurações de launch não entram no escopo de restauração de mods/configs da recuperação existente.

`LaunchSettingsPanel` usa tokens semânticos `--theme-*` com fallbacks oficiais e o identificador estável `data-theme-surface="java-memory-settings"`. A integração segue `docs/theme-integration.md` na entrega de temas: cores/apresentação não alteram settings, permissões ou comandos. Não há alteração do schema JSONC, IDs de navegação nem migração de temas salvos. O editor da instância e o painel global chamam o backend nativo. Os comandos globais recebem a memória máxima e a opção de incluir as personalizações. O diretório e a revisão são definidos pelo backend.

Assistente desktop, IPC e Core 1.1.5/Skins 0.3.1 foram preservados. Build/testes sintéticos não homologam Minecraft, loaders, Gemini, voz ou microfone. Evidências e roteiro restante: [tarefa Java/engine](task-java-engine.md).
