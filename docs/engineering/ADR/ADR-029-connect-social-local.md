# ADR-029 — Relações Aurora e catálogo social com testes locais

Data: 24/10/2026. Estado: **PROTÓTIPO** implementado e testado localmente; ingresso social e liberação publicada pendentes. Complementa R4-005 sem alterar prioridades, gates ou a matriz.

## Decisão

O nick pesquisado pertence ao cadastro `usernames` do Aurora. A consulta é exata, autenticada e não enumera contas ou jogadores Mojang. Amizade precisa de aceitação; pedidos, bloqueios e presença ficam em estruturas limitadas, validadas pelas regras do Firestore e por transações. A interface não decide a autorização por si só.

Sessão é uma declaração do anfitrião, com título, modpack, imagem permitida de catálogo, versão/loader e população declarada. A visibilidade começa privada; amigos e pública são opções explícitas, separadas da aprovação de ingresso. Nem anúncio nem presença comprovam um servidor alcançável, jogador no mundo ou áudio.

O catálogo público usa índice de IDs opacos e expiração, sem nick, UID, imagem, descrição, chave ou endereço. A projeção com metadados é lida individualmente, com autorização de visibilidade, amizade e bloqueio verificada nas regras. Negar essa leitura é uma decisão do backend; omitir a entrada recusada da tela não substitui autorização. Cada página é limitada a 20 candidatos, sem preenchimento recursivo. A implementação deve vincular a publicação ao dono e retirar o índice na restrição/fechamento, preservando negação de projeções antigas.

A presença por modpack é opcional e limitada no tempo. O cliente não publica automaticamente a lista de jogos nem a atividade pessoal. Não se introduz chat, importação de contatos, histórico de partidas ou conteúdo de áudio.

## Integração e limites

O SDK usa o Firebase Auth já existente. Não se amplia `functions` legado, não se adiciona serviço pago ou credencial administrativa. A configuração de teste usa somente Auth/Firestore Emulator, projeto `demo-aurora-social`, portas loopback fixas e identificador Tauri separado. O build normal mantém o social desligado. Nenhuma regra remota é publicada nesta decisão.

A autorização do transporte atual é estática e não equivale a política social. O botão **Entrar** deverá obter autorização vinculada à conta, chave comprovada, sessão e política atual, com consumo único e revogação. Não automatizar a aprovação manual existente nem tratar uma amizade como chave do transporte. Até esse novo caminho estar implementado/testado, a interface declara a entrada automática indisponível.

## Validação e liberação

Testar regras reais no emulador com casos positivos/negativos, SDK e interface. Registrar resultados no domínio social e tarefa. O emulador não prova limites, índices, latência ou cobrança da produção. Consultas diretas ainda precisam de proteção de frequência no serviço confiável e orçamento antes de liberar produção; limites de documentos não constituem rate limit global.

Fontes consultadas em 24/10/2026: [regras e transações](https://firebase.google.com/docs/firestore/security/rules-conditions), [consultas não são filtros](https://firebase.google.com/docs/firestore/security/rules-query), [emulador e projetos demo](https://firebase.google.com/docs/emulator-suite/connect_firestore). A execução local não usa recursos pagos.

Rollback: desligar o perfil social, encerrar os emuladores próprios e reverter somente os commits deste incremento. Não apagar dados pessoais, regras publicadas, perfis ou estágios anteriores. O transporte e as features Connect existentes permanecem separados.
