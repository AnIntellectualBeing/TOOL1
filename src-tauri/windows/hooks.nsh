; PhoneMirror installer hooks (Tauri NSIS).
;
; Adds two Windows Firewall rules so phones on the local network can reach
; PhoneMirror, and removes them on uninstall. In plain words:
;  * "PhoneMirror - iPhone receiver (UxPlay)": lets iPhones/iPads on your
;    Private (home) or Domain (work) network find this PC and send their
;    screen and sound to it (mDNS UDP 5353, then TCP/UDP 7100-7102).
;  * "PhoneMirror - find Android phones (mDNS)": lets PhoneMirror hear Android
;    phones announce themselves (UDP 5353) during wireless pairing.
; Public networks (cafes, airports) are NOT opened. The app has an
; "Allow on this network" button for people whose home Wi-Fi is set to Public.
;
; Keep the rule names in sync with crates/core/src/firewall.rs.

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Allowing PhoneMirror through Windows Firewall on private networks..."
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="PhoneMirror - iPhone receiver (UxPlay)"'
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="PhoneMirror - find Android phones (mDNS)"'
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="PhoneMirror - iPhone receiver (UxPlay)" dir=in action=allow enable=yes profile=private,domain program="$INSTDIR\engines\iphone\uxplay.exe" description="Lets iPhones and iPads on your local network find this PC and send their screen and sound to PhoneMirror."'
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="PhoneMirror - find Android phones (mDNS)" dir=in action=allow enable=yes profile=private,domain protocol=UDP localport=5353 program="$INSTDIR\${MAINBINARYNAME}.exe" description="Lets PhoneMirror find Android phones on your local network for wireless pairing."'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Removing PhoneMirror firewall rules..."
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="PhoneMirror - iPhone receiver (UxPlay)"'
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="PhoneMirror - find Android phones (mDNS)"'
  ; Stop the engines so their files can be removed.
  nsExec::ExecToLog 'taskkill /F /IM uxplay.exe'
  nsExec::ExecToLog 'taskkill /F /IM scrcpy.exe'
  nsExec::ExecToLog 'taskkill /F /IM adb.exe'
!macroend
