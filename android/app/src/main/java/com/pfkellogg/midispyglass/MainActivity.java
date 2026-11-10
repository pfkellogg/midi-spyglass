package com.pfkellogg.midispyglass;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.SharedPreferences;
import android.graphics.Color;
import android.media.midi.MidiManager;
import android.os.Bundle;
import android.util.TypedValue;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.widget.CheckBox;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.util.List;

/**
 * MIDI Spyglass: just the spyglass from FM-1 Mirror + Spyglass, for any MIDI keyboard or app. The brass eyepiece on
 * a dark screen shows the last note or chord played. Tap it, then ≡, for the MIDI connections, in pairs as in Synth
 * Spyglass: keyboards (FROM), each with the devices it plays (TO). Several pairs play at once; ⇄ on the dial shows
 * the next. Drag to move it; pinch, or tap and drag its grip, to resize it.
 */
public final class MainActivity extends Activity {
    private SharedPreferences prefs;
    private Inputs inputs;
    private Glass glass;
    /** The pair the spyglass shows. */
    private int current;

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        prefs = getSharedPreferences("spyglass", MODE_PRIVATE);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        FrameLayout stage = new FrameLayout(this);
        stage.setBackgroundColor(Color.rgb(8, 10, 13));
        setContentView(stage);
        if (android.os.Build.VERSION.SDK_INT >= 30) {
            WindowInsetsController c = getWindow().getInsetsController();
            if (c != null) {
                c.hide(WindowInsets.Type.systemBars());
                c.setSystemBarsBehavior(WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        } else {
            // (Android 10 and older, e.g. a Fire TV Stick on Fire OS 6: the older flags)
            stage.setSystemUiVisibility(android.view.View.SYSTEM_UI_FLAG_FULLSCREEN | android.view.View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                    | android.view.View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY | android.view.View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
        }
        glass = new Glass(this, prefs, this::chooseInputs, this::nextPair, () -> inputs == null ? null : inputs.fm1Sound());
        stage.addView(glass);
        // The remote (a TV): the glass takes focus; OK opens the MIDI connections, ◀ ▶ the other pairs.
        glass.setFocusable(true);
        glass.requestFocus();
        stage.post(() -> glass.place(stage.getWidth(), stage.getHeight()));
        MidiManager mm = getSystemService(MidiManager.class);
        if (mm == null) {
            glass.setIdle("");
            glass.setAlert("No MIDI found", "This device has no MIDI");
            return;
        }
        inputs = new Inputs(mm, this::showState);
        loadRoutes();
        current = prefs.getInt("current", 0);
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
        if (inputs != null) inputs.stop(); // another app can have the devices while this one is away
    }

    // ---- the pairs

    private void loadRoutes() {
        inputs.routes.clear();
        try {
            JSONArray a = new JSONArray(prefs.getString("routes", "[]"));
            for (int i = 0; i < a.length(); i++) {
                JSONObject o = a.getJSONObject(i);
                Inputs.Route r = new Inputs.Route();
                r.from = o.optString("from", "");
                JSONArray to = o.optJSONArray("to");
                if (to != null) for (int k = 0; k < to.length(); k++) r.to.add(to.getString(k));
                inputs.routes.add(r);
            }
        } catch (JSONException ignored) {
        }
        if (inputs.routes.isEmpty() && !prefs.contains("routes")) inputs.routes.add(new Inputs.Route()); // the first keyboard
    }

    private void saveRoutes() {
        JSONArray a = new JSONArray();
        for (Inputs.Route r : inputs.routes) {
            JSONObject o = new JSONObject();
            try {
                o.put("from", r.from);
                o.put("to", new JSONArray(r.to));
            } catch (JSONException ignored) {
            }
            a.put(o);
        }
        prefs.edit().putString("routes", a.toString()).putInt("current", current).apply();
    }

    private Inputs.Route route() {
        if (inputs.routes.isEmpty()) return null;
        if (current >= inputs.routes.size()) current = 0;
        return inputs.routes.get(current);
    }

    private void nextPair() {
        if (inputs == null || inputs.routes.size() < 2) return;
        current = (current + 1) % inputs.routes.size();
        saveRoutes();
        showState();
    }

    /** Shows the pair: its keyboard's notes, and in the header its name (with several) and what it plays. */
    private void showState() {
        Inputs.Route r = route();
        String in = r == null ? null : inputs.inputOf(r);
        boolean many = inputs.routes.size() > 1;
        boolean fm1 = false;
        if (r != null) {
            for (String t : inputs.playingTo(r)) if (Inputs.isFm1(t)) fm1 = true;
            if (inputs.routes.size() == 1 && r.to.isEmpty()) fm1 = true; // (one pair playing nothing: the FM-1 may be playing it)
        }
        glass.showPair(in == null ? null : inputs.played(in), many && in != null ? shortName(in) : null, fm1, many);
        glass.setIdle(in == null ? "" : "play a note");
        if (in != null) glass.setAlert(null, null);
        else glass.setAlert("No MIDI found", inputs.froms().isEmpty() ? "Plug in a USB or Bluetooth MIDI keyboard" : "Tap the glass, then ≡, to pick one");
    }

    /** "Keystation 49 MK3 (USB)" is "Keystation 49 MK3". */
    private static String shortName(String n) {
        int i = n.indexOf(" (");
        return i > 0 ? n.substring(0, i) : n;
    }

    // ---- the connections

    /**
     * The MIDI connections, as in Synth Spyglass: each keyboard (FROM) with a tick box; ticked, it's a pair, and the
     * devices it plays (TO) are ticked under it. Several pairs play at once. Also the waveform and the sound name.
     */
    private void chooseInputs() {
        if (inputs == null) return;
        // (the first keyboard, "", is named here: changing a pair makes it that keyboard for good)
        List<String> froms = inputs.froms();
        for (Inputs.Route r : inputs.routes) if (r.from.isEmpty() && !froms.isEmpty()) r.from = froms.get(0);
        LinearLayout list = new LinearLayout(this);
        list.setOrientation(LinearLayout.VERTICAL);
        int pad = Math.round(20 * getResources().getDisplayMetrics().density);
        list.setPadding(pad, pad / 2, pad, 0);
        ScrollView scroll = new ScrollView(this);
        scroll.addView(list);
        fill(list);
        new AlertDialog.Builder(this)
                .setTitle("MIDI: keyboards, each with its synths")
                .setView(scroll)
                .setNeutralButton(glass.wave() ? "Hide waveform" : "Show waveform", (d, w) -> glass.setWave(!glass.wave()))
                .setNegativeButton(glass.soundOn() ? "Hide sound name" : "Show sound name", (d, w) -> glass.setSoundOn(!glass.soundOn()))
                .setPositiveButton("Done", null)
                .show();
    }

    /** (Re)builds the list: the keyboards, and under each ticked one the devices it plays. */
    private void fill(LinearLayout list) {
        list.removeAllViews();
        List<String> froms = inputs.froms(), tos = inputs.tos();
        if (froms.isEmpty()) {
            TextView t = new TextView(this);
            t.setText("No MIDI keyboards. Plug in a USB keyboard, or pair a Bluetooth MIDI one in Android's settings.");
            list.addView(t);
            return;
        }
        int indent = Math.round(28 * getResources().getDisplayMetrics().density);
        for (String from : froms) {
            Inputs.Route found = null;
            for (Inputs.Route r : inputs.routes) if (r.from.equals(from)) found = r;
            final Inputs.Route pair = found;
            CheckBox f = new CheckBox(this);
            f.setText(from);
            f.setTextSize(TypedValue.COMPLEX_UNIT_SP, 17);
            f.setChecked(pair != null);
            f.setOnCheckedChangeListener((b, on) -> {
                if (on) {
                    Inputs.Route r = new Inputs.Route();
                    r.from = from;
                    inputs.routes.add(r);
                    current = inputs.routes.size() - 1; // (show the new pair)
                } else {
                    inputs.routes.remove(pair);
                    if (current >= inputs.routes.size()) current = Math.max(0, inputs.routes.size() - 1);
                }
                changed(list);
            });
            list.addView(f);
            if (pair == null) continue;
            TextView head = new TextView(this);
            head.setText("TO");
            head.setPadding(indent, 0, 0, 0);
            list.addView(head);
            int n = 0;
            for (String to : tos) {
                if (to.equals(from)) continue;
                CheckBox t = new CheckBox(this);
                t.setText(Inputs.isFm1(to) && !to.toLowerCase().contains("fm-1") ? to + " (FM-1)" : to);
                t.setChecked(pair.to.contains(to));
                LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT);
                lp.leftMargin = indent;
                t.setLayoutParams(lp);
                t.setOnCheckedChangeListener((b, on) -> {
                    if (on) pair.to.add(to);
                    else pair.to.remove(to);
                    changed(list);
                });
                list.addView(t);
                n++;
            }
            if (n == 0) {
                TextView none = new TextView(this);
                none.setText("No other MIDI devices to play");
                none.setPadding(indent, 0, 0, 0);
                list.addView(none);
            }
        }
    }

    private void changed(LinearLayout list) {
        saveRoutes();
        inputs.apply();
        list.post(() -> fill(list));
    }
}
