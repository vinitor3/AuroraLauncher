package net.minecraft.client.gui;
public final class GuiTextField {
    public static int created;
    public final int x, y;
    private String text = "";
    public GuiTextField(int id, FontRenderer font, int x, int y, int width, int height) { created++; this.x = x; this.y = y; }
    public void func_146203_f(int size) { }
    public void func_146180_a(String value) { text = value; }
    public String func_146179_b() { return text; }
    public void func_146178_a() { }
    public boolean func_146201_a(char character, int key) { text += character; return true; }
}
