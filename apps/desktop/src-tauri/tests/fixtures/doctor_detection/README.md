# Fixtures sintéticas do Doctor

Todos os arquivos desta pasta são sintéticos. Identificadores, classes e versões
foram inventados para testar os formatos, sem copiar logs de jogadores.
Eles não comprovam homologação no Minecraft.

Formatos confrontados com fontes primárias em 23/09/2026:

- [Mensagens do Fabric Loader](https://github.com/FabricMC/fabric-loader/blob/master/src/main/resources/net/fabricmc/loader/Messages.properties): dependências exigidas, recomendações opcionais e incompatibilidades.
- [ModSorter do Forge 1.20.x](https://github.com/MinecraftForge/MinecraftForge/blob/1.20.x/fmlloader/src/main/java/net/minecraftforge/fml/loading/ModSorter.java): linhas de dependência com versão instalada ausente ou incompatível.
- [Traduções do Forge 1.20.x](https://github.com/MinecraftForge/MinecraftForge/blob/1.20.x/src/main/resources/assets/forge/lang/en_us.json): pares de requisito e estado instalado no relatório de falha.
- [MixinProcessor](https://github.com/SpongePowered/Mixin/blob/master/src/main/java/org/spongepowered/asm/mixin/transformer/MixinProcessor.java): declaração de falha na aplicação de mixins.
- [GLFW/WGL](https://github.com/glfw/glfw/blob/master/src/wgl_context.c): falha na criação de contexto OpenGL.
- [JVMS 21, formato de classe](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-4.html): correspondência numérica entre versão de classe e Java.

As mensagens são dados não confiáveis. Os testes verificam que valores privados
plantados nas fixtures não chegam ao relatório e que uma menção ou frame isolado
não identifica a causa nem um mod responsável.
