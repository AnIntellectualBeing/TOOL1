# Testing PhoneMirror on real phones

The automated tests cover the logic (pairing, discovery, reconnection, engine
arguments, log parsing, firewall parsing) with a mock adb and a mock mDNS, and
CI checks that the bundled engines start on a clean Windows. What only real
hardware can tell us is below. Tick each line; if one fails, use
**Settings → Copy diagnostics** and paste the result into an issue together
with the step number.

**You need:** a Windows 10 or 11 PC on Wi-Fi (or Ethernet on the same router),
an iPhone or iPad, an Android phone (Android 11+ for wireless; any for USB), a
USB data cable. Ideally also an Android 10 phone and a second Wi-Fi network
(a phone hotspot is fine).

Write down: Windows version, PC model/GPU, phone models and OS versions.

---

## 0. Install

- [ ] 0.1 Download the installer from the Actions artifact or Release. SmartScreen
      warns; **More info → Run anyway** works.
- [ ] 0.2 The installer asks for administrator permission once and finishes.
- [ ] 0.3 In *Windows Security → Firewall → Advanced settings → Inbound rules*
      there are two rules: *PhoneMirror - iPhone receiver (UxPlay)* and
      *PhoneMirror - find Android phones (mDNS)*, profiles Private + Domain.
- [ ] 0.4 PhoneMirror opens. No console windows flash. Starting it a second
      time just brings the first window forward.
- [ ] 0.5 No "engine is missing" message appears.

## 1. iPhone (do this first)

- [ ] 1.1 The iPhone page says **Waiting for iPhone…** and shows
      *"<PC name> – PhoneMirror"*. Windows does **not** pop up its own firewall
      question (the installer's rule should cover it). If it does, note which
      program it names and what you clicked.
- [ ] 1.2 On the iPhone (same Wi-Fi): Control Center → Screen Mirroring. The PC's
      name appears in the list **within ~5 seconds**. *(Most important check:
      it proves UxPlay's built-in mDNS works on Windows without Bonjour.)*
- [ ] 1.3 Tap it. The iPhone screen appears in a window within ~3 s; PhoneMirror
      changes to **Showing <your iPhone's name>** with a green dot.
- [ ] 1.4 Sound: play a video. Sound comes out of the PC, in sync with the picture.
- [ ] 1.5 Rotate the iPhone to landscape (in a video or the Photos app). The
      window follows.
- [ ] 1.6 Latency: open the iPhone's stopwatch; delay feels under ~0.3 s.
- [ ] 1.7 **Fullscreen** button fills the screen; pressing it again restores.
      Alt+Enter in the picture window also toggles.
- [ ] 1.8 **Always on top** keeps the picture above other windows.
- [ ] 1.9 **Sound on/off** mutes and unmutes (check the Windows volume mixer
      shows *uxplay* muted).
- [ ] 1.10 **Screenshot** saves a PNG in *Pictures\PhoneMirror*; the toast's
      **Show** opens it. The PNG shows the iPhone picture (not black).
- [ ] 1.11 **Record**: while connected it says recording starts with the next
      session. Stop Mirroring on the iPhone, start again, use it ~20 s, Stop
      Mirroring. A toast says *iPhone recording saved*; the MP4 in
      *Videos\PhoneMirror* plays with picture **and** sound.
- [ ] 1.12 **Quality** → *Smooth*: applies at the next connection (smaller,
      smoother picture).
- [ ] 1.13 Stop Mirroring on the iPhone: the window closes and PhoneMirror goes
      back to **Waiting for iPhone…**.
- [ ] 1.14 **Speaker:** in the Music app (or any audio), tap the AirPlay icon,
      choose the PC. Audio plays on the PC; PhoneMirror says *Playing sound
      from …*. Turn on **Sound only**, repeat: no picture window opens.
- [ ] 1.15 **Video casting (experimental):** turn on *Allow video casting*.
      In the YouTube app tap the AirPlay/cast icon → the PC. Note whether the
      video plays, and the sound. (Known risk: some streaming plugins may be
      missing; diagnostics will say which.)
- [ ] 1.16 **PIN:** Settings → *Ask for a code before an iPhone can connect*.
      Connect: a 4-digit code shows in PhoneMirror and the iPhone asks for it.
- [ ] 1.17 **Rename:** Settings → Name shown on the iPhone → "Living Room". The
      iPhone list shows the new name (may take a few seconds).
- [ ] 1.18 **Public network:** in Windows set the Wi-Fi to *Public*. PhoneMirror
      warns that Windows treats it as public. Press **Allow on this network**,
      accept the Windows prompt; the iPhone finds the PC. Set it back to Private.
- [ ] 1.19 **Firewall repair:** delete the iPhone firewall rule by hand. The
      warning appears in Settings → Network & firewall; **Fix firewall** brings
      it back (one Windows prompt) and the iPhone connects again.
- [ ] 1.20 **Crash recovery:** end `uxplay.exe` in Task Manager. Within ~2 s
      PhoneMirror restarts it and the iPhone can connect again.
- [ ] 1.21 Video output: Settings → *Direct3D 12* and *Compatible* both show a
      picture (note any that's black or crashes, with your GPU model).

## 2. Android over USB

- [ ] 2.1 Turn on Developer options and USB debugging (the wizard's *Use a USB
      cable instead* shows how for your brand). Plug in. PhoneMirror lists the
      phone with a **USB** badge; before you tap *Allow* on the phone it says
      *Your phone is asking for permission*.
- [ ] 2.2 Press **Mirror**. The phone window opens within ~3 s; status shows
      *Mirroring*.
- [ ] 2.3 Mouse: click, scroll, drag. Keyboard: type in a text field. Right
      click = Back.
- [ ] 2.4 Sound plays on the PC (Android 11+). On Android 10, PhoneMirror shows
      *Sound needs Android 11 or newer* and mirroring continues.
- [ ] 2.5 **Copy/paste:** copy text on the PC, Ctrl+V in a phone text field;
      copy on the phone, paste on the PC.
- [ ] 2.6 **Drop a photo** onto the PhoneMirror window: overlay appears; toast
      says it's in the phone's Download folder; check in the phone's Files app.
- [ ] 2.7 **Drop an APK**: it installs (toast *installed*). On Xiaomi, if it
      fails, the message mentions *Install via USB*.
- [ ] 2.8 **Keep the phone's screen off**: phone screen turns off, mirroring
      continues.
- [ ] 2.9 **Screenshot** saves a full-resolution PNG of the phone screen.
- [ ] 2.10 **Record** → use the phone ~20 s → **Stop recording**. Toast *Recording
      saved*; the MP4 plays with sound.
- [ ] 2.11 **Rotate**, **Fullscreen**, **Always on top**, **Sound on/off**,
      **Quality** each work (rotate/quality briefly reconnect; that's expected).
- [ ] 2.12 **Camera:** *Show → Back camera* (Android 12+): the camera picture
      appears. On Android 11 or older PhoneMirror explains it needs Android 12.
- [ ] 2.13 **Unplug the cable** while mirroring: status shows *Reconnecting
      (attempt n of 6)*; plug back in within ~30 s and it resumes by itself.
- [ ] 2.14 Close the phone window yourself: PhoneMirror shows *Not connected*,
      no error.

## 3. Android wireless pairing (QR)

- [ ] 3.1 **Add a phone** → choose your brand. The pictures and menu names match
      your phone closely enough to follow (note any that don't, with the phone
      model and OS version: **this is the most useful feedback for the wizard**).
- [ ] 3.2 On the phone: Wireless debugging → **Pair device with QR code**, scan
      the code. PhoneMirror goes *Found your phone → Pairing → Connecting →
      ✅ connected* within ~10 s.
- [ ] 3.3 **Start mirroring** works exactly like USB (repeat 2.3–2.5 quickly).
- [ ] 3.4 Close PhoneMirror, reopen it. The phone is listed under *Your phones*.
      Press **Connect**: it mirrors without any pairing.
- [ ] 3.5 Turn Wireless debugging **off and on** on the phone (the port
      changes). **Connect** still works (found by name over mDNS).
- [ ] 3.6 Restart the phone, don't turn Wireless debugging on, press **Connect**:
      message *We can't find <phone> on this Wi-Fi… Wireless debugging…*.
- [ ] 3.7 Put the phone on another network (e.g. mobile hotspot from a second
      phone, or a guest Wi-Fi) and press **Connect**: message *Your phone and PC
      are on different Wi-Fi networks* or *can't find it on this Wi-Fi*.
- [ ] 3.8 While mirroring wirelessly, turn the phone's Wi-Fi off for ~10 s and
      on again: PhoneMirror reconnects by itself.
- [ ] 3.9 **Pair with a code instead:** on another phone (or after *Forget*), use
      *Pair device with pairing code* and type the address and code. It pairs.
- [ ] 3.10 Wrong code: message *Pairing didn't work* with a **Show a new code**
      button.
- [ ] 3.11 **Forget** removes the phone; *Revoke USB debugging authorisations*
      on the phone then **Connect** (before forgetting) says the phone doesn't
      recognise this PC anymore.
- [ ] 3.12 Windows did not ask any firewall question during pairing (the
      installer's mDNS rule). If it did, note it.
- [ ] 3.13 Xiaomi/OPPO only: without the extra security option, clicking shows
      *Your phone blocks mouse and keyboard control* with the right advice.

## 4. Networks that block devices

- [ ] 4.1 On a network with "client/AP isolation" (many guest networks): iPhone
      doesn't see the PC and Android pairing times out with the hotspot advice.
- [ ] 4.2 Same test using a **phone hotspot** (PC connected to it, iPhone or
      Android on it as a client, or the Android phone itself as the hotspot):
      everything works.

## 5. Both at once & housekeeping

- [ ] 5.1 Mirror an iPhone and an Android phone at the same time.
- [ ] 5.2 Close PhoneMirror while mirroring: both engine windows close, no
      `uxplay.exe`, `scrcpy.exe` left in Task Manager (adb may stay; that's normal).
- [ ] 5.3 Protected content: Netflix on Android shows black (expected).
- [ ] 5.4 Uninstall from *Settings → Apps*: files and both firewall rules are gone.
- [ ] 5.5 Leave mirroring running 30 minutes: no growing delay, no crash.
