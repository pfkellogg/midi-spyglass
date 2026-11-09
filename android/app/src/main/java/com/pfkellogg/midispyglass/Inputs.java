package com.pfkellogg.midispyglass;

import android.media.midi.MidiDevice;
import android.media.midi.MidiDeviceInfo;
import android.media.midi.MidiInputPort;
import android.media.midi.MidiManager;
import android.media.midi.MidiOutputPort;
import android.media.midi.MidiReceiver;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;

import java.io.Closeable;
import java.io.IOException;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Every MIDI device that sends notes (a USB keyboard, a Bluetooth one once Android has paired it, another
 * app's virtual port), all at once, into one Played. Devices that come and go are picked up as they do.
 * The only thing sent is a hello to an FM-1 with Big Screen firmware, about once a second: that's what makes it
 * say the name of the sound playing ("T1 FM6 PIANO 1"), as it does for FM-1 Mirror + Spyglass.
 */
final class Inputs {
    private final MidiManager mm;
    private final Played played;
    private final Runnable changed;
    private final Handler ui = new Handler(Looper.getMainLooper());
    /** Open devices by id, and the names of those turned off. */
    private final Map<Integer, Open> open = new HashMap<>();
    final Set<String> skip = new HashSet<>();

    private static final class Open implements Closeable {
        final MidiDeviceInfo info;
        final MidiDevice device;
        final List<MidiOutputPort> ports = new ArrayList<>();
        /** An FM-1's MIDI in: where the hellos go. */
        MidiInputPort hello;

        Open(MidiDeviceInfo info, MidiDevice device) {
            this.info = info;
            this.device = device;
        }

        @Override
        public void close() {
            if (hello != null) {
                try {
                    hello.close();
                } catch (IOException ignored) {
                }
            }
            for (MidiOutputPort p : ports) {
                try {
                    p.close();
                } catch (IOException ignored) {
                }
            }
            try {
                device.close();
            } catch (IOException ignored) {
            }
        }
    }

    private final MidiManager.DeviceCallback watch = new MidiManager.DeviceCallback() {
        @Override
        public void onDeviceAdded(MidiDeviceInfo info) {
            consider(info);
        }

        @Override
        public void onDeviceRemoved(MidiDeviceInfo info) {
            Open o = open.remove(info.getId());
            if (o != null) o.close();
            changed.run();
        }
    };

    Inputs(MidiManager mm, Played played, Runnable changed) {
        this.mm = mm;
        this.played = played;
        this.changed = changed;
    }

    private static final byte[] HELLO = {(byte) 0xf0, 0x7d, 'F', 'S', 0, (byte) 0xf7};
    private static final String[] FM1_HINTS = {"fm-1", "fm1", "felucca", "melodee", "choralroot", "loom-8", "jiant"};

    static boolean isFm1(MidiDeviceInfo i) {
        String n = name(i).toLowerCase(java.util.Locale.US);
        for (String h : FM1_HINTS) if (n.contains(h)) return true;
        return false;
    }

    /** Hello to each FM-1, about once a second (it stops saying its sound three seconds after the last). */
    private final Runnable greet = new Runnable() {
        @Override
        public void run() {
            for (Open o : open.values()) {
                if (o.hello == null) continue;
                try {
                    o.hello.send(HELLO, 0, HELLO.length);
                } catch (IOException ignored) {
                }
            }
            ui.postDelayed(this, 900);
        }
    };

    void start() {
        mm.registerDeviceCallback(watch, ui);
        for (MidiDeviceInfo i : mm.getDevices()) consider(i);
        ui.post(greet);
    }

    void stop() {
        ui.removeCallbacks(greet);
        mm.unregisterDeviceCallback(watch);
        for (Open o : open.values()) o.close();
        open.clear();
    }

    /** Every device that sends MIDI, open or not. */
    List<MidiDeviceInfo> all() {
        List<MidiDeviceInfo> found = new ArrayList<>();
        for (MidiDeviceInfo i : mm.getDevices()) if (i.getOutputPortCount() > 0 && !isSelf(i)) found.add(i);
        return found;
    }

    int listening() {
        return open.size();
    }

    /** Turns one on or off by name (remembered by the caller), and opens or closes it now. */
    void set(MidiDeviceInfo info, boolean on) {
        String n = name(info);
        if (on) skip.remove(n);
        else skip.add(n);
        if (!on) {
            Open o = open.remove(info.getId());
            if (o != null) o.close();
            changed.run();
        } else {
            consider(info);
        }
    }

    private void consider(MidiDeviceInfo info) {
        if (info.getOutputPortCount() == 0 || isSelf(info) || skip.contains(name(info)) || open.containsKey(info.getId())) return;
        mm.openDevice(info, device -> {
            if (device == null) return;
            if (open.containsKey(info.getId()) || skip.contains(name(info))) {
                try {
                    device.close();
                } catch (IOException ignored) {
                }
                return;
            }
            Open o = new Open(info, device);
            for (int k = 0; k < info.getOutputPortCount(); k++) {
                MidiOutputPort port = device.openOutputPort(k);
                if (port == null) continue;
                Played.Stream stream = played.stream(name(info));
                port.connect(new MidiReceiver() {
                    @Override
                    public void onSend(byte[] data, int offset, int count, long timestamp) {
                        stream.feed(data, offset, count);
                    }
                });
                o.ports.add(port);
            }
            if (isFm1(info) && info.getInputPortCount() > 0) o.hello = device.openInputPort(0);
            open.put(info.getId(), o);
            changed.run();
        }, ui);
    }

    private static boolean isSelf(MidiDeviceInfo i) {
        Bundle p = i.getProperties();
        return p != null && "com.pfkellogg.midispyglass".equals(p.getString(MidiDeviceInfo.PROPERTY_NAME));
    }

    static String name(MidiDeviceInfo i) {
        Bundle p = i.getProperties();
        String n = p == null ? null : p.getString(MidiDeviceInfo.PROPERTY_NAME);
        if (n == null || n.isEmpty()) n = p == null ? null : p.getString(MidiDeviceInfo.PROPERTY_PRODUCT);
        if (n == null || n.isEmpty()) n = "MIDI device " + i.getId();
        String how = i.getType() == MidiDeviceInfo.TYPE_BLUETOOTH ? "Bluetooth" : i.getType() == MidiDeviceInfo.TYPE_USB ? "USB" : "app";
        return n + " (" + how + ")";
    }
}
