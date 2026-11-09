package com.pfkellogg.midispyglass;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.SharedPreferences;
import android.graphics.Color;
import android.media.midi.MidiDeviceInfo;
import android.media.midi.MidiManager;
import android.os.Bundle;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.widget.FrameLayout;

import java.util.HashSet;
import java.util.List;

/**
 * MIDI Spyglass: just the spyglass from FM-1 Mirror + Spyglass, for any MIDI keyboard or app. The brass
 * eyepiece on a dark screen shows the last note or chord played on every MIDI input Android has.
 * Tap it, then ≡, to choose the inputs; drag to move it; pinch, or tap and drag its grip, to resize it.
 */
public final class MainActivity extends Activity {
    private SharedPreferences prefs;
    private Inputs inputs;
    private Glass glass;

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        prefs = getSharedPreferences("spyglass", MODE_PRIVATE);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        FrameLayout stage = new FrameLayout(this);
        stage.setBackgroundColor(Color.rgb(8, 10, 13));
        setContentView(stage);
        WindowInsetsController c = getWindow().getInsetsController();
        if (c != null) {
            c.hide(WindowInsets.Type.systemBars());
            c.setSystemBarsBehavior(WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
        }
        Played played = new Played();
        glass = new Glass(this, played, prefs, this::chooseInputs);
        stage.addView(glass);
        stage.post(() -> glass.place(stage.getWidth(), stage.getHeight()));
        MidiManager mm = getSystemService(MidiManager.class);
        if (mm == null) {
            glass.setIdle("");
            glass.setAlert("No MIDI found", "This device has no MIDI");
            return;
        }
        inputs = new Inputs(mm, played, this::showState);
        inputs.skip.addAll(prefs.getStringSet("skip", new HashSet<>()));
        showState();
    }

    @Override
    protected void onStart() {
        super.onStart();
        if (inputs != null) inputs.start();
        // The FM-1's real sound for the waveform comes over its USB cable as an audio input: Android calls that the microphone.
        if (Scope.findDevice(this) != null && checkSelfPermission(android.Manifest.permission.RECORD_AUDIO) != android.content.pm.PackageManager.PERMISSION_GRANTED)
            requestPermissions(new String[] {android.Manifest.permission.RECORD_AUDIO}, 8);
    }

    @Override
    protected void onStop() {
        super.onStop();
        if (inputs != null) inputs.stop(); // another app can have the keyboard while this one is away
    }

    private void showState() {
        boolean none = inputs.listening() == 0;
        glass.setIdle(none ? "" : "play a note");
        if (!none) glass.setAlert(null, null);
        else glass.setAlert("No MIDI found", inputs.all().isEmpty() ? "Plug in a USB or Bluetooth MIDI keyboard" : "Tap the glass, then ≡, to turn an input on");
    }

    /** The MIDI inputs, ticked when the spyglass listens to them. */
    private void chooseInputs() {
        if (inputs == null) return;
        List<MidiDeviceInfo> all = inputs.all();
        if (all.isEmpty()) {
            new AlertDialog.Builder(this)
                    .setTitle("MIDI inputs")
                    .setMessage("No MIDI inputs. Plug in a USB keyboard, or pair a Bluetooth MIDI one in Android's settings.")
                    .setNeutralButton(glass.wave() ? "Hide waveform" : "Show waveform", (d, w) -> glass.setWave(!glass.wave()))
                    .setPositiveButton("OK", null)
                    .show();
            return;
        }
        String[] names = new String[all.size()];
        boolean[] on = new boolean[all.size()];
        for (int i = 0; i < all.size(); i++) {
            names[i] = Inputs.name(all.get(i));
            on[i] = !inputs.skip.contains(names[i]);
        }
        new AlertDialog.Builder(this)
                .setTitle("Listening to")
                .setMultiChoiceItems(names, on, (d, which, checked) -> {
                    inputs.set(all.get(which), checked);
                    prefs.edit().putStringSet("skip", new HashSet<>(inputs.skip)).apply();
                })
                .setNeutralButton(glass.wave() ? "Hide waveform" : "Show waveform", (d, w) -> glass.setWave(!glass.wave()))
                .setNegativeButton(glass.soundOn() ? "Hide sound name" : "Show sound name", (d, w) -> glass.setSoundOn(!glass.soundOn()))
                .setPositiveButton("Done", null)
                .show();
    }
}
