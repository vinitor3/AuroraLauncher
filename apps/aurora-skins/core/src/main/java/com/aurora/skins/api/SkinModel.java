package com.aurora.skins.api;

public enum SkinModel {
    WIDE("classic"), SLIM("slim");

    private final String id;
    SkinModel(String id) { this.id = id; }
    public String id() { return id; }

    public static SkinModel fromId(String value) {
        return "slim".equalsIgnoreCase(value) ? SLIM : WIDE;
    }
}
