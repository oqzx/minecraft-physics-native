package dev.minecraftphysics.oracle;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.minecraft.client.MinecraftClient;
import net.minecraft.client.option.KeyBinding;
import net.minecraft.client.network.ClientPlayerEntity;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;

public final class OracleClientCapture implements ClientModInitializer {
    private static final int PORT = Integer.getInteger("oracle.client.port", 25566);
    private static final BlockingQueue<String> INPUTS = new LinkedBlockingQueue<>();
    private static volatile PrintWriter output;
    private static volatile boolean armed;
    private static boolean readySent;
    private static boolean tickInputApplied;
    private static int tick;

    @Override
    public void onInitializeClient() {
        ClientTickEvents.START_CLIENT_TICK.register(OracleClientCapture::beforeTick);
        ClientTickEvents.END_CLIENT_TICK.register(OracleClientCapture::afterTick);
        Thread listener = new Thread(OracleClientCapture::listen, "oracle-client-capture");
        listener.setDaemon(true);
        listener.start();
    }

    private static void listen() {
        try (ServerSocket server = new ServerSocket(PORT)) {
            while (!server.isClosed()) {
                try (Socket socket = server.accept();
                     BufferedReader reader = new BufferedReader(new InputStreamReader(
                         socket.getInputStream(), StandardCharsets.UTF_8))) {
                    output = new PrintWriter(socket.getOutputStream(), true, StandardCharsets.UTF_8);
                    readySent = false;
                    tick = 0;
                    armed = false;
                    tickInputApplied = false;
                    String line;
                    while ((line = reader.readLine()) != null) {
                        INPUTS.put(line);
                    }
                } catch (IOException exception) {
                    System.err.println("[oracle-client-capture] " + exception.getMessage());
                } catch (InterruptedException exception) {
                    Thread.currentThread().interrupt();
                    return;
                } finally {
                    INPUTS.offer("{\"type\":\"stop\"}");
                    armed = false;
                    output = null;
                }
            }
        } catch (IOException exception) {
            System.err.println("[oracle-client-capture] Cannot listen on port " + PORT + ": " + exception.getMessage());
        }
    }

    private static void beforeTick(MinecraftClient client) {
        if (!armed || client.player == null) return;
        try {
            JsonObject command = JsonParser.parseString(INPUTS.take()).getAsJsonObject();
            if ("stop".equals(command.get("type").getAsString())) {
                armed = false;
                setControls(client, new JsonObject());
                return;
            }
            setControls(client, command);
            tickInputApplied = true;
        } catch (InterruptedException exception) {
            Thread.currentThread().interrupt();
            armed = false;
        }
    }

    private static void afterTick(MinecraftClient client) {
        if (output == null || client.player == null) return;
        if (!readySent) {
            readySent = true;
            armed = true;
            emit("ready", client.player, -1);
        } else if (tickInputApplied) {
            emit("tick", client.player, tick++);
            tickInputApplied = false;
        }
    }

    private static void setControls(MinecraftClient client, JsonObject command) {
        set(client.options.forwardKey, command, "forward");
        set(client.options.backKey, command, "back");
        set(client.options.leftKey, command, "left");
        set(client.options.rightKey, command, "right");
        set(client.options.jumpKey, command, "jump");
        set(client.options.sprintKey, command, "sprint");
        set(client.options.sneakKey, command, "sneak");
    }

    private static void set(KeyBinding key, JsonObject command, String name) {
        key.setPressed(command.has(name) && command.get(name).getAsBoolean());
    }

    private static void emit(String type, ClientPlayerEntity player, int tickNumber) {
        PrintWriter writer = output;
        if (writer == null) return;
        JsonObject state = new JsonObject();
        state.addProperty("x", player.getX());
        state.addProperty("y", player.getY());
        state.addProperty("z", player.getZ());
        state.addProperty("velocityX", player.getVelocity().x);
        state.addProperty("velocityY", player.getVelocity().y);
        state.addProperty("velocityZ", player.getVelocity().z);
        state.addProperty("yawDegrees", player.getYaw());
        state.addProperty("yaw", Math.toRadians(player.getYaw()));
        state.addProperty("pitch", Math.toRadians(player.getPitch()));
        state.addProperty("onGround", player.isOnGround());
        state.addProperty("inWater", player.isTouchingWater());
        state.addProperty("inLava", player.isInLava());
        state.addProperty("fallDistance", player.fallDistance);
        state.addProperty("halfWidth", player.getWidth() / 2.0);
        state.addProperty("height", player.getHeight());
        JsonObject message = new JsonObject();
        message.addProperty("type", type);
        message.addProperty("tick", tickNumber);
        message.add("state", state);
        writer.println(message);
    }
}