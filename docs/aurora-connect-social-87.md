# Aurora Connect — amigos e comunidade no laboratório local (87)

Data: 24/10/2026. Estado: **PROTÓTIPO** local, **IMPLEMENTADO MAS NÃO HOMOLOGADO** para uso publicado. Este incremento implementa relações de amizade, busca pelo nick do Aurora, atividade compartilhada e anúncios de sessões. **Entrar sem copiar convite ainda é NÃO IMPLEMENTADO**: o botão permanece desabilitado, com a limitação visível. Não confundir descoberta de uma sessão com autorização do transporte.

Contrato de produto: [experiência social](aurora-connect-social-experience.md). Decisão técnica: [ADR-029](engineering/ADR/ADR-029-connect-social-local.md). Comandos, registros e falhas: [tarefa 87](task-aur-r4-connect-social-87.md). Pertence a AUR-R4-005, preservando os demais gates, prioridades, Assistente desktop e nove pares Core/Skins.

## O que foi implementado

| Capacidade | Comportamento deste incremento |
| --- | --- |
| Busca de pessoas | Consulta explícita pelo nick completo do cadastro Aurora, de 3 a 16 letras, números ou `_`, sem diferenciar maiúsculas/minúsculas na chave. Retorna no máximo uma conta. Não consulta Mojang, jogadores Minecraft, prefixos ou uma lista global de usuários. |
| Pedidos e amizade | Enviar, aceitar, recusar e cancelar pedidos; remover amizade; bloquear e desbloquear. Aceitação precisa da outra pessoa e a mudança recíproca é validada atomicamente. Desbloquear não restaura amizade. |
| Privacidade | O grafo social de outra conta não é legível. Bloqueio impede relações e leitura de presença/anúncios do par. Consulta de coleções de contas/perfis é negada; permanecem leituras pontuais autorizadas. |
| Atividade | Compartilhamento opcional para amigos, inicialmente oculto. A interface recebe do launcher o nome de uma instância em execução e só publica após ação explícita. Não usa a instância apenas selecionada como prova de jogo ativo. Não há publicação ou renovação automática. |
| Descoberta | Abas **Comunidade** e **Dos amigos**, filtro sobre os anúncios já carregados, visualização em grade ou lista. Cartões médios mostram título, modpack, versão, loader e população declarada. |
| Imagem | Campo opcional de imagem de catálogo, validado contra os CDNs permitidos; qualquer origem arbitrária é recusada. Sem imagem válida, a interface mostra uma capa com iniciais. A primeira rodada nativa usou fallback. Uma rodada posterior com CSP limitada aos três CDNs permitidos comprovou uma imagem Modrinth real em grade e lista; carregamento dos CDNs Forge não foi observado. |
| Publicação | Anúncio próprio começa **Privado**. **Meus amigos** e **Toda a comunidade** exigem escolha e confirmação explícitas. **Entrada aberta** e **Aprovar cada pedido** são campos separados de visibilidade. Salvar um anúncio não inicia Minecraft, abre portas, modifica whitelist ou concede acesso à ponte. |
| Entrada | **Entrar** ou **Pedir para entrar** aparece conforme a política, mas fica desabilitado porque `capabilities.join=false`. O adaptador recusa `prepareJoin`; não há sucesso simulado nem aprovação automática do laboratório existente. |

As referências visuais do usuário orientam a organização da lista de amigos e dos cartões, sem importar marca, contatos, chat ou estatísticas de terceiros. Avatares arbitrários não são carregados; a lista usa iniciais. O nome de modpack na presença é uma declaração recente compartilhada com amigos, não prova de jogador dentro do mundo, autenticação Minecraft ou voz.

## Dados e autorização

`apps/desktop/src/shared/social.ts` define as projeções e validações. `apps/desktop/src/lib/socialFirebase.ts` usa o SDK existente, com a identidade autenticada capturada na criação do adaptador. Cada etapa posterior confere essa identidade; a troca de conta impede uma resposta atrasada de iniciar a próxima leitura ou publicar um resultado na tela.

As regras em `firebase/firestore.rules` validam as operações sobre estruturas separadas:

- `socialState`: listas privadas limitadas de amigos, pedidos recebidos/enviados e bloqueados; o par é alterado atomicamente sem ler o grafo privado do destinatário no cliente.
- `socialRequests` e `socialThrottle`: prazo original do pedido e intervalo de novas solicitações, conferidos pelo servidor; aceitação expirada é recusada. Recusar/cancelar pode retirar pedidos expirados e liberar a cota.
- `socialPresence`: atividade para amigos ou oculta, com prazo limitado. Não há histórico de partidas nem enumeração de presença.
- `socialOwners` e `socialSessions`: no máximo um anúncio vinculado ao dono; projeção legível somente conforme visibilidade, amizade, bloqueio e prazo.
- `socialCatalog`: índice paginado de IDs opacos e expiração, sem nick, UID do anfitrião, título, imagem, endereço, convite ou chave. Cada projeção é obtida separadamente e passa pela autorização das regras. A recusa de uma projeção não é substituída por filtragem visual.

Publicar, restringir e fechar o anúncio conserva atomicamente o vínculo do dono e o índice. Alterar para privado ou fechar retira a entrada pública; conhecer um ID antigo não torna uma projeção privada legível. Nenhum documento social contém chave privada, token de autenticação, destino local ou convite de transporte.

| Limite atual | Aplicação |
| --- | --- |
| 100 amigos e 100 bloqueados por conta | Listas privadas limitadas pelas regras. |
| 10 pedidos recebidos e 10 enviados | Limite de pendências; limpeza explícita de expirados. |
| Até 7 dias para um pedido | Prazo original validado contra o horário do servidor; não renova na leitura ou aceitação. |
| 2 segundos entre novas solicitações de amizade | Proteção dessa escrita, não rate limit global do serviço. |
| Até 120 segundos de presença | Renovação somente por ação explícita neste incremento. |
| Até 300 segundos de anúncio | Expiração lógica; leitura não renova, nem mantém o jogo hospedado. |
| 20 candidatos por página pública | Sem busca recursiva para preencher recusas/expirados. |
| 100 anúncios retidos na interface | Limite do conjunto carregado, mesmo após paginação. |
| 15 segundos por operação do adaptador | Após timeout, o contexto fica inválido mesmo com recuo do relógio. O SDK pode já ter enviado uma escrita; a interface manda atualizar antes de repetir. |

Campos de descoberta são texto limitado; React não interpreta título como HTML. URLs de imagem aceitam apenas HTTPS nos hosts `cdn.modrinth.com`, `media.forgecdn.net` e `mediafilez.forgecdn.net`, sem credenciais, porta explícita, consulta ou fragmento e com extensão de imagem permitida. O navegador não recebe liberdade para buscar avatares em origens escolhidas por outro usuário.

## Interface e isolamento

`SocialView` recebe um serviço vinculado à conta. Conta ou serviço diferente desmonta a árvore anterior antes de apresentar outro usuário; gerações descartam respostas antigas. Busca ocorre somente ao enviar o formulário. Operações simultâneas/repetidas são bloqueadas enquanto uma ação e sua atualização estão pendentes. Uma atualização que falha após mutação retira projeções antigas acionáveis.

Pedidos expirados exibem **Pedido expirado**, desabilitam **Aceitar** e conservam **Recusar/Cancelar pedido** para limpeza. A aceitação também reconfere o relógio no clique. Anúncios vencidos desaparecem e presença vencida deixa de mostrar o modpack. O relógio é revisto periodicamente e ao retomar foco/visibilidade, sem inferir que um temporizador constitui estado do backend.

O perfil `social-lab` fixa os dados/autenticação sociais em Auth Emulator e Firestore Emulator locais, projeto `demo-aurora-social`; não aceita configuração salva de outro projeto e remove endpoints de Worker da configuração social. O executável usa identificador `com.aurora.smartlauncher.sociallab`, separado do launcher normal. Isso **não é isolamento global de rede do aplicativo**: funcionalidades nativas preexistentes, como a consulta de versões Java em `available_java_versions`/`engine/jre`, podem consultar `api.adoptium.net` por HTTP nativo fora da CSP. Essas consultas não são promovidas a prova de transferência ou de ausência de tráfego nesta rodada. Nenhuma regra remota, serviço pago, faturamento, publicação ou instalador é acionado. O build normal conserva o serviço social desligado.

Existe também `apps/desktop/social-lab.html`, uma página de teste com o mesmo `SocialView`, autenticação real do emulador e o mesmo adaptador/regras. Não usa respostas de rede simuladas nem cria amizades, contas ou anúncios ao abrir. Os botões de conta aceitam somente três fixtures locais previamente preparadas. Esse harness depende do modo de build local e de endereço loopback; não é uma alternativa para autenticar em produção.

Cada login do harness possui um app Firebase e escopo não persistente próprios. **Encerrar sessão local** aguarda sign-out, encerramento do Firestore e exclusão do app SDK. Se qualquer fase falhar, novas entradas naquela aba ficam bloqueadas; a mensagem não declara cleanup aprovado. Fechar uma aba oferece apenas limpeza de melhor esforço, distinta do encerramento explícito observado.

Depois da rodada GUI congelada, o aviso observado de raiz React duplicada em HMR recebeu correção própria no preview: um proprietário por geração desmonta a raiz anterior e conserva a promessa original de cleanup SDK; a nova geração espera essa barreira. Falha de cleanup bloqueia as gerações seguintes, em vez de reiniciar o laboratório. Login em andamento é aguardado antes de destruir o SDK, e uma geração encerrada não inicia a leitura de perfil posterior. Essa correção foi exercitada por seis testes causais com raízes React em DOM sintético e promessas controladas, sem Firebase/rede. Não reatribuir o ensaio GUI anterior ao preview corrigido nem declarar hot reload real com contas novamente homologado.

## Evidências executadas

As camadas são separadas; não somar repetições como novas capacidades:

| Camada | Resultado observado |
| --- | --- |
| SDK e regras Firestore reais | 18 testes PASS, 0 falhas, 6.021,6117 ms, com contextos autenticados de teste e SDK contra o emulador. Inclui relações, bloqueio, privacidade, anúncio/índice, expiração, limites, tentativas malformadas e recusa de conta Auth sem cadastro Aurora. A rodada anterior de 17 PASS em 5.582,0716 ms é subconjunto, não resultado adicional. Não é autenticação real de produção. |
| Fronteira causal do cliente | 3 testes PASS, 333,5363 ms, fixtures separadas de mudança de conta durante leitura, timeout com recuo do relógio e validações de projeção. Não são tráfego real de Firestore. |
| Regras anteriores | 7 testes PASS, 975,4074 ms; repetição independente pelo responsável: os mesmos 7 PASS, 980,2737 ms. Não somar como 14. |
| DOM combinado final | 262 PASS, 0 falhas/ignorados, 3.298,4839 ms: 25 sociais, 6 HMR, 4 de isolamento Firebase/CSP e 227 Connect existentes. A rodada anterior255PASS/3.616,6494ms permanece registrada, sem somar as repetições. Fixtures DOM não comprovam transporte, login real ou Minecraft. |
| Lifecycle HMR focal posterior | 6 PASS, 0 falhas/ignorados, 584,663 ms; sucessão de raízes React, cleanup atrasado/falho, geração obsoleta e dispose idempotente em DOM sintético. TypeScript PASS. Esses seis casos foram depois incluídos no lote262; não somar262+6. Separado da GUI anterior. |
| Frontend normal | Build PASS; 378 módulos, aviso de chunk maior que 500 kB preservado. |
| Executável social local | Build debug sem bundle PASS, 11,60 s de compilação nativa final; 378 módulos frontend. Não é instalador ou release. |

A cópia privada do executável usado está em `.aurora/connect-validation/social-87-native/AuroraSocialLab.exe`, 64.704.512 bytes, SHA-256 `ad366e84b4c86f6b14c8f6e128ab3b3b5d6cf6940d079b1476dd860672bc2f33`.

O responsável executou a interface de navegador com contas locais descartáveis: primeira conta buscou a segunda pelo nick Aurora e enviou pedido; segunda conta aceitou; a amizade apareceu. A segunda publicou voluntariamente um anúncio público com nome, versão e população declarada `0/8`; a primeira viu o cartão e a visualização em lista. Bloquear a segunda removeu amizade e anúncio da projeção da primeira. São operações efetivas do Auth/Firestore Emulator pela interface, não fixtures DOM.

No executável completo, o responsável entrou com a terceira conta local pelo Auth Emulator, observou o anúncio público mesmo sem amizade, buscou a segunda e enviou pedido. A segunda aceitou pelo navegador; a atualização nativa mostrou um amigo, nenhum pedido enviado e a relação reconhecida na busca. Depois, a segunda restringiu explicitamente o anúncio de público para amigos. A atualização nativa retirou o cartão de **Comunidade** e o apresentou em **Dos amigos**, com modpack, versão, loader e rótulo **Para amigos**. O anúncio não corresponde, por esta prova, a um mundo Minecraft iniciado ou acessível.

Ao terminar, a segunda conta usou **Encerrar anúncio**, com confirmação; a atualização nativa em **Dos amigos** passou a **Nenhuma sessão disponível**. As duas sessões do harness foram encerradas explicitamente e apresentaram **autenticação e conexão de dados liberadas**. No executável, **Sair da conta** retornou à tela de login; nenhum Connect foi iniciado nessa rodada. Depois do logout, o responsável encerrou forçadamente somente o processo no caminho exato da nova cópia SocialLab, confirmado ausente depois; a janela Connect anterior ficou intacta. **Não é prova de saída normal do launcher** (`normalApplicationExitVerified=false`).

As sessões próprias de Vite e Firebase CLI receberam Ctrl+C e terminaram com exit1; não se registra saída graciosa normal por esse código. Consulta posterior não encontrou listeners TCP nas portas locais 8187, 9187, 1437, 4487, 4587 e 9150. É prova da ausência desses listeners naquele momento, não um rebind explícito nem prova individual de reap/exit0 de todos os possíveis filhos do emulador. A tarefa preserva os avisos/falhas anteriores do desenvolvimento e distingue encerramento social normal de encerramento operacional dos processos de laboratório.

### Rodada posterior — capa de catálogo no executável

Somente a CSP do perfil nativo mudou entre os pins runtime conferidos pelo integrador: `img-src` passou a admitir os três CDNs já aceitos pelo contrato de imagem, mantendo `connect-src` social loopback. Build social debug sem bundle PASS, compilação nativa13,80s e Vite488ms. A cópia `.aurora/connect-validation/social-87-artwork-native/AuroraSocialLab.exe` tem64.704.512B e SHA256 `25801e971f42ca991f84fa9c57879f0a5ec24bc0422c5623ad9d9910f431d495`. A rodada anterior com o exe `ad366…2f33` permanece separada e válida para seus próprios fluxos.

O integrador usou uma imagem pública do Fabric API, obtida a partir da API oficial Modrinth, como **capa de fixture**: `https://cdn.modrinth.com/data/P7dR8mSH/icon.png`. Isso não declara Fabric API como modpack nem como jogo homologado. O anúncio desta subrodada foi criado e fechado pelo SDK autenticado do projeto demo, através de fixture privada; não se atribui a publicação dessa subrodada à interface.

Na interface nativa, a terceira conta local autenticou, atualizou o catálogo e mostrou a imagem efetivamente carregada em **Grade** e **Lista**, com versão/população e **Entrar** desabilitado. Depois do fechamento do anúncio pela fixture, a atualização mostrou **Nenhuma sessão disponível**. A imagem privada `social-87-native-artwork.png` registra a observação. Somente esse CDN/imagem foi exercitado; os dois hosts Forge continuam sem observação de carga nesta rodada. Os testes de isolamento Firebase/CSP passaram 4/4 em 528,2245 ms, sendo três repetidos e um novo; não são quatro provas novas além do lote255.

O integrador observou logout até a tela de login e encerrou forçadamente somente uma instância no caminho exato do novo executável de imagem. A primeira consulta ainda contou um processo em transição; a consulta posterior confirmou zero. O Firebase CLI próprio recebeu Ctrl+C e terminou exit1, sem prova de saída graciosa. Consulta final encontrou zero listeners TCP nas mesmas seis portas locais; Vite não foi iniciado nesta subrodada. A janela Connect anterior permaneceu intacta. Essa evidência pertence à subrodada nova e não substitui a precisão ou as falhas da anterior; continua sem prova de saída normal do launcher ou rebind explícito das portas.

## Limites e próximos passos

O ingresso social continua o próximo contrato de transporte: vincular conta autenticada a chave comprovada, aplicar amizade/bloqueio/política/capacidade atuais, obter autorização individual, consumi-la e revogá-la, sem copiar convite e sem afrouxar o fluxo manual. É necessário testar corrida de bloqueio/mudança de política com admissão e conexão ativa. O trabalho de amigos não torna o registro de serviços uma ponte aberta.

O catálogo direto ainda não possui proteção global confiável de frequência ou orçamento de produção. Uma página de até 20 IDs pode exigir até 20 leituras adicionais de projeções, inclusive recusas. Na capacidade máxima estrutural, um snapshot pode tentar até 764 leituras lógicas de documentos: estado próprio (1), amigos (100×5), pedidos (20×3), bloqueados (100×2) e presença/ponteiro/sessão próprios (3). Campos ausentes/recusados e o prazo de 15 segundos reduzem o trabalho efetivo; esse cálculo não é quantidade comprovada de requisições de rede nem orçamento de cobrança, e exclui leituras dependentes das regras. Não foi feito benchmark de todas as listas na capacidade máxima. Limite de página/documento não impede repetição maliciosa e não garante custo zero em produção. TTL lógico não equivale a exclusão física/retention job homologado. Esses bloqueios mantêm a liberação pública desligada; não habilitar produção apenas porque o emulador passou.

Atividade não se renova automaticamente, presença confiável de conexão/jogador e sincronização entre janelas continuam pendentes. Publicação é um formulário de anúncio declarado, ainda sem vínculo a uma sessão de transporte/jogo viva. A prova de imagem remota é limitada à fixture Modrinth acima; saída normal completa do processo launcher, redes distintas e uso com contas reais publicadas não são promovidos por este incremento.

| Domínio | Estado após 87 |
| --- | --- |
| TCP | Provas anteriores preservadas; nenhum novo transporte TCP de jogo neste incremento. |
| UDP | Provas anteriores preservadas; nenhum UDP de aplicação novo. |
| Direto | Sem nova prova IPv4/IPv6 externa, NAT/CGNAT ou seleção automática por desempenho. |
| Relay | Desligado; nenhum novo serviço ou custo. |
| Minecraft | Nenhum novo servidor, login, mundo, dois jogadores ou homologação Core/Skins. |
| Amigos, convites e interface | Amigos/busca/anúncios **PROTÓTIPO** testado localmente; ingresso social sem cópia **NÃO IMPLEMENTADO**. Convites e transporte anteriores permanecem separados. |
| Simple Voice Chat | Sem novo anúncio, autenticação ou áudio; evidências anteriores conservam seus limites. |
| Plasmo Voice | Sem novo anúncio, autenticação ou áudio; evidências anteriores conservam seus limites. |

Rollback: manter o perfil normal desligado, encerrar as sessões locais e os emuladores próprios e reverter somente o incremento revisado se necessário. Não apagar bases pessoais, marcadores/estágios antigos, regras publicadas ou o rascunho histórico de voz. Nenhuma intervenção humana é necessária para continuar os incrementos locais independentes; produção e homologação externa permanecem etapas separadas.
