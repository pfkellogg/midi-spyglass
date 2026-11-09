# MIDI Spyglass

The spyglass from [FM-1 Mirror](https://github.com/pfkellogg/fm1-mirror), on its own, for any MIDI keyboard or
app. A round brass eyepiece floats over everything and shows what you play:

- the **note** or **chord** name (C major, Cmaj7, …), big
- the notes on a **grand staff**
- a **waveform** of the notes, beside the staff
- the full name, the pitch in Hz, and **how hard** the key was hit
- the **sound**, across the top: the General MIDI name of the last Program Change on that input and channel
  (for example *Electric Piano 1 · 5*), or the keyboard's name until one comes

No FM-1 needed. It listens to **every MIDI input** the computer or tablet has, all at once, and picks up a keyboard
the moment it's plugged in. It only listens: it never sends anything.

![MIDI Spyglass showing C major](docs/spyglass.png)

## Download

Mac, Windows, Linux and Android: see [Releases](../../releases).

| | |
| --- | --- |
| **Mac** | `MIDI-Spyglass-mac.zip`: unzip, drag *MIDI Spyglass* to Applications. The first time, right-click it and choose Open. |
| **Windows** | `MIDI-Spyglass-windows.exe`: run it (Windows may ask you to confirm an app from the internet). |
| **Linux** | `MIDI-Spyglass-linux`: `chmod +x` it and run it (needs ALSA). |
| **Android** | `MIDI-Spyglass-android.apk`: install it (Android 11 or newer, with MIDI). |

## Using it

**Mac, Windows, Linux**
- **Drag** the spyglass to move it.
- **Resize** it with the brass grip on its rim (bottom right), or scroll over it.
- **Right-click** it for the MIDI inputs it listens to (untick one to ignore it), *Sound name*, *Waveform*,
  *Always on top*, and *Quit*.

**Android**
- **Drag** it to move it, **pinch** it to resize it.
- **Tap** it for its two buttons: the grip (drag to resize) and **≡**: the MIDI inputs, and the sound name and
  waveform on or off.
- USB keyboards work straight away; a Bluetooth MIDI keyboard once it's paired in Android's settings.

Where it is and its size are remembered.

## About the sound name and the waveform

MIDI carries notes, not sound. So:
- The **sound name** comes from Program Change messages. Its General MIDI name is right for General MIDI
  synths and apps; other synths number their sounds their own way, so the number after the name is the one to
  trust there.
- The **waveform** is drawn from the notes themselves: a sine at each note's pitch, added up, so one note is a
  clean wave and a chord shows its combined shape. It isn't your synth's actual sound.

## Building

- Desktop (Rust): `cd desktop && cargo run --release`. On a Mac, `./make_app.sh` builds *MIDI Spyglass.app*
  (Apple silicon and Intel) and installs it in /Applications. Linux needs `libasound2-dev`.
- Android: `cd android && ./gradlew assembleRelease` (JDK 17).
- `tools/make_icon.py` draws the icon (Pillow).

## License

GPL-3.0, like FM-1 Mirror, which the spyglass comes from (it follows Synth Spyglass's look).
Contact: pfkellogg@gmail.com
