# Changelog

## 1.2.0 (2026-11-10)

- **Pairs**: several keyboards (FROM), each with the synths it plays (TO), all at once. The spyglass shows one pair
  at a time; the **⇄ switch on the dial** shows the next, and the header says whose.
- **Soft synths** (Mac): Analog Lab V and the other Arturia apps, Splice INSTRUMENT, Pianoteq… listed under TO by name;
  ticking one ticks the bus it listens to and opens it. The IAC row says who listens ("IAC Driver Bus 1: Analog Lab V").
- **A soft synth's real waveform** (Mac, macOS 14.2+), through Synth Spyglass's `synth-tap` helper. Layered, the louder
  sound shows.
- The header follows what the pair plays: the FM-1's sound, the soft synth's name, or both.
- **Android**: the same pairs, the ≡ menu by keyboard, and the switch on the dial.

## 1.1.0 (2026-11-09)

- **Click the glass for its MIDI connections**, filling its face: FROM (what's shown, and what plays) and TO (the
  synths it plays, layered when several are ticked). MIDI Spyglass became a bridge too: notes to an FM-1 go on
  channel 1, and nothing goes back to the port it came from.
- The waveform shows only while one is playing (no flat line).
- Audio works on a thread of its own, so a macOS permission prompt never freezes the spyglass.

## 1.0.0 (2026-11-09)

- The spyglass from FM-1 Mirror on its own: the note or chord, the grand staff, the pitch, the velocity, for any
  MIDI keyboard. Mac, Windows, Linux and Android.
- The waveform: the FM-1's real sound from its USB audio, or drawn from the notes (a still 20 ms window, like MIDI
  Bridge's spyglass).
- The sound's name: an FM-1's Big Screen firmware says it (a hello once a second), else a Program Change's General
  MIDI name.
- **No MIDI found** when there's nothing to listen to.
