package com.pfkellogg.midispyglass;

import java.util.Arrays;

/**
 * What's been played, for the spyglass: the last note and how hard, and the keys held now (for
 * chord names), fed from MIDI threads: one Stream per input, each with its own running status.
 */
final class Played {
    private final boolean[] down = new boolean[128];
    private int lastNote = -1, lastVelocity, firstVelocity = -1;
    private boolean velocityVaries;
    private long noteOns;
    /** The sound each input's channel last picked (Program Change), and where the last note came from. */
    private final java.util.Map<String, int[]> programs = new java.util.HashMap<>();
    private int lastChannel;
    private String lastSource;

    Stream stream(String source) { return new Stream(source); }

    /** One input's running status; SysEx passes through untouched. */
    final class Stream {
        private final String source;
        private int status, d1, have;

        Stream(String source) {
            this.source = source;
        }

        void feed(byte[] m, int offset, int count) {
            synchronized (Played.this) {
                for (int i = offset; i < offset + count; i++) {
                    int b = m[i] & 0xff;
                    if (b >= 0xf8) continue; // clock and the like
                    if (b >= 0x80) {
                        status = b < 0xf0 ? b : 0;
                        have = 0;
                        continue;
                    }
                    if (status == 0) continue;
                    int type = status & 0xf0;
                    if (type == 0xd0) continue; // nothing to track
                    if (type == 0xc0) { // one data byte: the sound picked
                        int[] pr = programs.get(source);
                        if (pr == null) {
                            pr = new int[16];
                            java.util.Arrays.fill(pr, -1);
                            programs.put(source, pr);
                        }
                        pr[status & 15] = b;
                        continue;
                    }
                    if (have == 0) {
                        d1 = b;
                        have = 1;
                        continue;
                    }
                    have = 0; // running status: the next pair reuses it
                    if (type == 0x90 && b > 0) {
                        lastChannel = status & 15;
                        lastSource = source;
                    }
                    event(type, d1, b);
                }
            }
        }
    }

    private void event(int type, int note, int value) {
        if (type == 0x90 && value > 0) {
            down[note] = true;
            lastNote = note;
            lastVelocity = value;
            if (firstVelocity < 0) firstVelocity = value;
            else if (value != firstVelocity) velocityVaries = true;
            noteOns++;
        } else if (type == 0x80 || type == 0x90) {
            down[note] = false;
        } else if (type == 0xb0 && (note == 123 || note == 120)) {
            Arrays.fill(down, false);
        }
    }

    synchronized long noteOns() { return noteOns; }

    /** What's playing: the sound the last note's channel picked on its input, if a Program Change said; else the input. */
    synchronized String sound() {
        if (lastSource == null) return null;
        int[] pr = programs.get(lastSource);
        return pr != null && pr[lastChannel] >= 0 ? Gm.soundName(lastChannel, pr[lastChannel]) : lastSource;
    }

    synchronized int lastNote() { return lastNote; }

    /** How hard the last key was hit; -1 until the keyboard has sent two different ones (some always send the same). */
    synchronized int velocity() { return velocityVaries ? lastVelocity : -1; }

    synchronized int[] keysDown() {
        int n = 0;
        for (boolean d : down) if (d) n++;
        int[] k = new int[n];
        n = 0;
        for (int i = 0; i < 128; i++) if (down[i]) k[n++] = i;
        return k;
    }
}
