package net.minecraftforge.client.event;

import java.util.List;

/** Test event shape; production is separately compiled against the real Forge JAR. */
public class GuiScreenEvent {
    private final Object gui;
    private boolean canceled;
    GuiScreenEvent(Object gui) { this.gui = gui; }
    public Object getGui() { return gui; }
    public void setCanceled(boolean value) { canceled = value; }
    public boolean isCanceled() { return canceled; }
    public static class InitGuiEvent extends GuiScreenEvent {
        private final List<Object> buttons;
        InitGuiEvent(Object gui, List<Object> buttons) { super(gui); this.buttons = buttons; }
        public List<Object> getButtonList() { return buttons; }
        public static class Post extends InitGuiEvent { public Post(Object gui, List<Object> buttons) { super(gui, buttons); } }
    }
    public static class ActionPerformedEvent extends GuiScreenEvent {
        private final Object button;
        ActionPerformedEvent(Object gui, Object button) { super(gui); this.button = button; }
        public Object getButton() { return button; }
        public static class Pre extends ActionPerformedEvent { public Pre(Object gui, Object button) { super(gui, button); } }
    }
    public static class DrawScreenEvent extends GuiScreenEvent {
        DrawScreenEvent(Object gui) { super(gui); }
        public int getMouseX() { return 0; }
        public int getMouseY() { return 0; }
        public float getRenderPartialTicks() { return 0F; }
        public static class Pre extends DrawScreenEvent { public Pre(Object gui) { super(gui); } }
        public static class Post extends DrawScreenEvent { public Post(Object gui) { super(gui); } }
    }
    public static class KeyboardInputEvent extends GuiScreenEvent {
        KeyboardInputEvent(Object gui) { super(gui); }
        public static class Pre extends KeyboardInputEvent { public Pre(Object gui) { super(gui); } }
    }
    public static class MouseInputEvent extends GuiScreenEvent {
        MouseInputEvent(Object gui) { super(gui); }
        public static class Pre extends MouseInputEvent { public Pre(Object gui) { super(gui); } }
    }
}
