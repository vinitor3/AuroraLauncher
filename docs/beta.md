# Aurora Smart Launcher 0.2.0-beta.2

| Área | O que mudou |
| --- | --- |
| Modpacks Modrinth | O importador respeita o lado do cliente e a prioridade de `client-overrides`. Atualizações de instalações antigas removem os arquivos exclusivos de servidor que o pacote instalou. O histórico permite reverter a atualização. |
| Conta | Vincule um e-mail nas opções da conta e confirme o link. O mesmo perfil passa a usar o e-mail como login. Em **Esqueci minha senha**, solicite a mensagem para definir outra senha. A conta precisa ter um e-mail confirmado antes da perda da senha. |
| Capas | Escolha um PNG no guarda-roupa, confira a prévia e aplique a capa. A seleção fica disponível localmente; alterações pendentes podem ser sincronizadas com a conta. A opção **Mostrar minha capa a outros jogadores** requer Aurora Skins compatível no servidor e nos clientes. |
| Teclas | Abra **Teclas** na instância. O mapa ABNT2/ANSI inclui mouse, numérico e comandos do pack. Em Forge 1.12.2, 1.16.5, 1.19.2 e 1.20.1 também dá para atribuir Alt, Ctrl ou Shift. No Forge 1.21.1 da matriz, o jogo falha ao reler esse formato; no Fabric, combinações dependem do mod de controles. O editor permite alterar a tecla simples nesses perfis. |
| Jogar junto | Nos convites Aurora, o anfitrião convida um amigo para a sessão privada. O convidado aceita pelo launcher. Para sessões públicas, as permissões e os pedidos de aprovação continuam valendo. |
| Discord | A presença é opcional. Em **Amigos e comunidade → Convites → Presença no Discord**, use o ID público do aplicativo Discord do projeto. Uma sessão pública anunciada com vaga habilita o convite. Os participantes precisam do Discord desktop e do Aurora abertos. Esta distribuição ainda depende do cadastro de um ID oficial para que todos possam ativá-la com o mesmo aplicativo. |

## Minecraft e loaders

| Minecraft | Fabric | Forge | Java do jogo |
| --- | :---: | :---: | --- |
| 1.12.2 | — | Sim | 8 |
| 1.16.5 | Sim | Sim | 8 |
| 1.19.2 | Sim | Sim | 17 |
| 1.20.1 | Sim | Sim | 17 |
| 1.21.1 | Sim | Sim | 21 |
| 1.21.11 | Sim | — | 21 |
| 26.3 | Sim | — | 25 |

A matriz embarca Core 1.1.7 e Skins 0.3.4. No teste de migração com Adrenaline 26.5.0 → 26.5.1, a atualização retirou dois arquivos exclusivos de servidor. A reversão restaurou os 36 arquivos ao conteúdo anterior e preservou um arquivo adicionado pelo jogador. A recuperação de senha foi exercitada com uma conta de teste nos emuladores oficiais do Firebase. A verificação de posse do Minecraft pela Microsoft permanece prevista para o final do projeto.

[Guia rápido](README.md) · [Matriz exata](../scripts/active-mod-matrix.json) · [Instalador e integridade](https://github.com/vinitor3/AuroraLauncher/releases/tag/v0.2.0-beta.2) · [Histórico da beta.1](https://github.com/vinitor3/AuroraLauncher/releases/tag/v0.2.0-beta.1)
