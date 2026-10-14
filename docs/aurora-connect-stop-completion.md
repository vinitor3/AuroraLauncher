# Encerramento retido até o fim do worker — correção71a

Estado: **PROTÓTIPO**, correção do ciclo nativo experimental. Complementa o [caller desktop](aurora-connect-desktop.md) e preserva as autoridades, prazos e perfis existentes. Não altera transporte, configuração de Minecraft, mods ou dados pessoais.

## Falha observada

A regressão integrada69–71 encontrou uma sequência real: o ator já havia fechado seus recursos e publicado `closed/complete`, mas sua tarefa ainda não havia retornado. `stop` classificava a tarefa como não terminal, sobrescrevia o estado para `closing/pending` e aguardava seu retorno. Como o ator já tinha publicado o resultado final, o caller devolvia `pending` sem outra publicação. A mesma sequência podia afetar o resultado `failed/complete`.

Transferir uma tarefa ainda pendente para a future de uma chamada IPC também retirava essa propriedade do manager. Se a chamada fosse abandonada, outra chamada de encerramento podia deixar de aguardar o mesmo worker.

## Correção

O manager conserva a publicação final e mantém o `JoinHandle` pendente no `Attempt`. Chamadas de stop duplicadas e logout observam e aguardam o mesmo worker, mesmo se o primeiro solicitante desaparecer. O handle só é retirado quando a tarefa terminou; seu resultado ainda é recolhido, e falha da tarefa mantém o estado de falha de cleanup. Nenhum await ocorre sob a mutex.

Novo preparo continua bloqueado enquanto o worker ou a consulta própria estiverem ativos. Cleanup completo e término da tarefa são condições distintas; nenhum snapshot autoriza reutilizar recursos antes do término efetivo. O latch de logout permanece fechado, sem reiniciar uma sessão automaticamente.

## Prova e limites

Dois testes causais controlam a cauda do worker após a publicação final, nas variantes de sucesso e erro. Cobrem stop duplicado, logout, perda da primeira future, manutenção do snapshot e recusa de reutilização do slot. Não dependem de vencer uma corrida por tempo ou velocidade do PC; não abrem transporte nem executam Java.

Esses dois testes passaram e a regressão original com TCP e duas origens UDP voltou a passar sem reduzir sua asserção. Rodadas completas e comandos estão no [registro71a](task-aur-r4-connect.md#incremento71a--encerramento-sem-regredir-o-resultado-publicado). Compilação e testes locais não homologam janela gráfica, jogo, áudio ou redes externas.

## Rollback

Encerrar as operações próprias e manter o estado final observado. Reverter somente a correção em branch revisável restauraria a corrida conhecida; não usar downgrade como recuperação de uma operação viva. Nenhuma configuração pessoal, contrato de convite, versão, dependência ou matriz precisa de migração.
