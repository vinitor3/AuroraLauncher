package com.aurora.skins.minecraft.fabric;

import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.Identifier;

/** The channel body is the common protocol, with no loader-specific length or discriminator. */
public record SkinSyncPayload(byte[] bytes) implements CustomPacketPayload {
    static final int MAX_BYTES = 24640;
    public static final Type<SkinSyncPayload> TYPE =
            new Type<>(Identifier.fromNamespaceAndPath("aurora_skins", "sync"));
    public static final StreamCodec<RegistryFriendlyByteBuf, SkinSyncPayload> CODEC = new StreamCodec<>() {
        @Override public SkinSyncPayload decode(RegistryFriendlyByteBuf buffer) {
            int size = buffer.readableBytes();
            if (size < 1 || size > MAX_BYTES) {
                buffer.skipBytes(size);
                return new SkinSyncPayload(new byte[0]);
            }
            byte[] bytes = new byte[size];
            buffer.readBytes(bytes);
            return new SkinSyncPayload(bytes);
        }

        @Override public void encode(RegistryFriendlyByteBuf buffer, SkinSyncPayload payload) {
            if (payload.bytes.length < 1 || payload.bytes.length > MAX_BYTES) {
                throw new IllegalArgumentException("Invalid Aurora Skins payload size");
            }
            buffer.writeBytes(payload.bytes);
        }
    };

    @Override public Type<? extends CustomPacketPayload> type() { return TYPE; }
}
