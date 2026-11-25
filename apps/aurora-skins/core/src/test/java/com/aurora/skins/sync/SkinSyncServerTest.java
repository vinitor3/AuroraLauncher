package com.aurora.skins.sync;

import com.aurora.skins.api.SkinModel;
import org.junit.jupiter.api.Test;
import javax.imageio.ImageIO;
import java.awt.image.BufferedImage;
import java.io.*;
import java.util.*;
import java.util.zip.CRC32;
import static org.junit.jupiter.api.Assertions.*;

class SkinSyncServerTest {
    private final UUID alice = UUID.randomUUID(), bob = UUID.randomUUID(), charlie = UUID.randomUUID();
    private final SkinSyncServer server = new SkinSyncServer();
    private final Map<UUID, List<byte[]>> inbox = new HashMap<UUID, List<byte[]>>();
    private final SkinSyncServer.Sender sender = (id, packet) -> inbox.computeIfAbsent(id, key -> new ArrayList<byte[]>()).add(packet);
    private void receive(UUID id, byte[] packet, long time) { server.receive(id, packet, time, sender); }
    private void hello(UUID id) { receive(id, SkinSyncProtocol.hello(), 10000); }
    static byte[] png(int height, int color) throws IOException {
        BufferedImage image = new BufferedImage(64, height, BufferedImage.TYPE_INT_ARGB);
        for (int y = 0; y < height; y++) for (int x = 0; x < 64; x++) image.setRGB(x,y,color);
        ByteArrayOutputStream output = new ByteArrayOutputStream(); ImageIO.write(image,"png",output); return output.toByteArray();
    }
    private SkinSyncProtocol.Message last(UUID id) throws IOException {
        List<byte[]> packets = inbox.get(id); return SkinSyncProtocol.decode(packets.get(packets.size()-1));
    }

    @Test void capesAreOptionalBoundToTheConnectionAndContainOnlyModelPixels() throws Exception {
        hello(alice); hello(bob); hello(charlie);
        byte[] cape = SkinSyncProtocol.capePublish(png(32, 0xffbe145e));
        receive(alice, cape, 11000);
        assertEquals(1, inbox.get(bob).size());
        receive(alice, SkinSyncProtocol.capeHello(), 12000);
        receive(bob, SkinSyncProtocol.capeHello(), 12000);
        receive(alice, cape, 13000);
        SkinSyncProtocol.Message update = last(bob);
        assertEquals(SkinSyncProtocol.CAPE_UPDATE, update.kind);
        assertEquals(alice, update.player);
        BufferedImage pixels = ImageIO.read(new ByteArrayInputStream(update.png()));
        assertEquals(0xffbe145e, pixels.getRGB(5, 9));
        assertEquals(0, pixels.getRGB(40, 20));
        assertEquals(1, inbox.get(charlie).size(), "Old clients receive no cape packets");
        receive(bob, SkinSyncProtocol.capeUpdate(alice, update.png()), 14000);
        assertEquals(SkinSyncProtocol.CAPE_HELLO, last(alice).kind);
        receive(charlie, SkinSyncProtocol.capeHello(), 15000);
        assertEquals(alice, last(charlie).player, "Late peers receive an authenticated snapshot");
        receive(alice, SkinSyncProtocol.reset(), 16000);
        assertEquals(SkinSyncProtocol.CAPE_UPDATE, last(bob).kind, "Changing body skin preserves cape");
        server.disconnect(alice, sender);
        assertEquals(SkinSyncProtocol.CAPE_REMOVE, last(bob).kind);
        assertThrows(IOException.class, () -> SkinSyncProtocol.canonicalCape(png(64, 0xff112233)));
    }

    @Test void distributesSkinsByConnectionIdentityAndSnapshotsToLateJoiners() throws Exception {
        hello(alice); hello(bob);
        byte[] skin = png(64,0xff123456);
        receive(alice, SkinSyncProtocol.publish(SkinModel.SLIM,skin), 11000);
        SkinSyncProtocol.Message update = last(bob);
        assertEquals(SkinSyncProtocol.UPDATE, update.kind); assertEquals(alice,update.player);
        assertEquals(SkinModel.SLIM,update.model); assertArrayEquals(SkinSyncProtocol.canonicalPng(skin),update.png());
        assertEquals(1,inbox.get(alice).size(),"Sender keeps its own local texture");
        hello(charlie); assertEquals(alice,last(charlie).player);
        receive(bob,SkinSyncProtocol.publish(SkinModel.WIDE,png(32,0xff765432)),14000);
        assertEquals(bob,last(alice).player); assertEquals(SkinModel.WIDE,last(alice).model);
        assertEquals(64,ImageIO.read(new ByteArrayInputStream(last(alice).png())).getHeight());
    }

    @Test void cannotPublishForAnotherPlayerOrBeforeHandshake() throws Exception {
        hello(bob);
        receive(alice,SkinSyncProtocol.publish(SkinModel.WIDE,png(64,0)),11000);
        assertEquals(1,inbox.get(bob).size());
        hello(alice);
        receive(alice,SkinSyncProtocol.update(bob,SkinModel.WIDE,png(64,0)),12000);
        receive(alice,SkinSyncProtocol.remove(bob),15000);
        assertEquals(1,inbox.get(bob).size());
    }

    @Test void changesModelEvenWhenPixelsMatchAndLimitsRepeatedUpdates() throws Exception {
        hello(alice); hello(bob); byte[] skin=png(64,0xff123456);
        receive(alice,SkinSyncProtocol.publish(SkinModel.WIDE,skin),11000);
        receive(alice,SkinSyncProtocol.publish(SkinModel.SLIM,skin),11500);
        assertEquals(SkinModel.WIDE,last(bob).model);
        receive(alice,SkinSyncProtocol.publish(SkinModel.SLIM,skin),13500);
        assertEquals(SkinModel.SLIM,last(bob).model);
        int count=inbox.get(bob).size();
        receive(alice,SkinSyncProtocol.publish(SkinModel.SLIM,skin),16000);
        assertEquals(count,inbox.get(bob).size(),"Identical pixels and model are deduplicated");
    }

    @Test void resetLeaveAndServerStopRemoveState() throws Exception {
        hello(alice); hello(bob);
        receive(alice,SkinSyncProtocol.publish(SkinModel.WIDE,png(64,0)),11000);
        receive(alice,SkinSyncProtocol.reset(),14000);
        assertEquals(SkinSyncProtocol.REMOVE,last(bob).kind); assertEquals(alice,last(bob).player);
        receive(alice,SkinSyncProtocol.publish(SkinModel.SLIM,png(64,0)),17000);
        server.disconnect(alice,sender);
        assertEquals(SkinSyncProtocol.REMOVE,last(bob).kind); assertEquals(1,server.playerCount());
        server.clear(); assertEquals(0,server.playerCount());
        hello(charlie); assertEquals(1,inbox.get(charlie).size(),"No old-server skin survives restart");
    }

    @Test void rejectsMalformedAndOversizedPacketsAndChargesBadPngAttempts() throws Exception {
        hello(alice); hello(bob);
        byte[] valid = SkinSyncProtocol.publish(SkinModel.WIDE,png(64,0));
        for (int length=0;length<valid.length;length++) {
            final byte[] truncated=Arrays.copyOf(valid,length);
            assertThrows(IOException.class,()->SkinSyncProtocol.decode(truncated));
        }
        assertThrows(IOException.class,()->SkinSyncProtocol.decode(new byte[SkinSyncProtocol.MAX_PACKET_BYTES+1]));
        byte[] badModel=valid.clone(); badModel[2]=2;
        assertThrows(IOException.class,()->SkinSyncProtocol.decode(badModel));
        byte[] bad=valid.clone(); bad[bad.length-1]^=1;
        receive(alice,bad,11000); receive(alice,valid,11500);
        assertEquals(1,inbox.get(bob).size());
        receive(alice,valid,13500); assertEquals(2,inbox.get(bob).size());
    }

    @Test void rejectsHugeDimensionsBeforeDecodingAndChecksPngCrc() throws Exception {
        byte[] image=png(64,0);
        image[16]=0x7f; image[17]=(byte)0xff; image[18]=(byte)0xff; image[19]=(byte)0xff;
        CRC32 crc=new CRC32(); crc.update(image,12,17);
        long checksum=crc.getValue(); for(int i=0;i<4;i++) image[29+i]=(byte)(checksum >>> (24-i*8));
        assertThrows(IOException.class,()->SkinSyncProtocol.canonicalPng(image));
        byte[] corrupt=png(64,0); corrupt[corrupt.length-1]^=1;
        assertThrows(IOException.class,()->SkinSyncProtocol.canonicalPng(corrupt));
    }

    @Test void boundsSubscribersAndRecoversSlotsOnDisconnect() {
        for(int i=0;i<SkinSyncProtocol.MAX_PLAYERS+20;i++) hello(new UUID(0,i));
        assertEquals(SkinSyncProtocol.MAX_PLAYERS,server.playerCount());
        server.disconnect(new UUID(0,0),sender); hello(alice);
        assertEquals(SkinSyncProtocol.MAX_PLAYERS,server.playerCount()); assertNotNull(inbox.get(alice));
    }
}
