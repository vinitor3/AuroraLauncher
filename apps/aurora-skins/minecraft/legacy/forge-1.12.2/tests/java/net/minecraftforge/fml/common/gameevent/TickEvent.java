package net.minecraftforge.fml.common.gameevent;
public class TickEvent {
    public enum Phase { START, END }
    public final Phase phase;
    TickEvent(Phase phase) { this.phase = phase; }
    public static class ClientTickEvent extends TickEvent { public ClientTickEvent(Phase phase) { super(phase); } }
    public static class RenderTickEvent extends TickEvent { public RenderTickEvent(Phase phase) { super(phase); } }
}
