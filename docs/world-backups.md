# Backups dos mundos

Abra **Gerenciar → Backups de mundos** para escolher o padrão global ou ativar e desativar o backup só nessa instância. O padrão inicial é ativado. A mudança afeta as próximas capturas e preserva o histórico existente.

Quando uma partida iniciada pelo Aurora termina, o launcher verifica os mundos de um jogador em `saves`, inclusive os que já existiam. Se você fechar o launcher primeiro, ele retoma a verificação ao reabrir, depois que o jogo sair. Enquanto o backup ou a restauração estiver em andamento, aguarde para abrir a mesma instância. Um erro aparece no painel com a opção **Tentar backup novamente**.

Selecione um mundo, uma versão e **Restaurar como novo mundo**. O Aurora verifica os arquivos, monta a cópia e só então a disponibiliza. Ela conserva o nome mostrado pelo Minecraft e recebe uma pasta própria em `saves`. A versão que você já joga continua disponível.

O histórico local fica em `.aurora/world-backups` dentro da instância. Renomear a instância mantém o histórico. Excluir a instância remove também seus backups locais. Cada instância guarda até cinco versões por mundo e 128 versões no total. O limite por captura é de 8 GiB e 65.536 entradas. O arquivo de histórico admite até 8 GiB de blocos únicos, além do índice. Quando os limites ou o espaço livre impedem uma captura, o último histórico concluído permanece disponível. Blocos iguais de 1 MiB são reutilizados entre arquivos e versões.
