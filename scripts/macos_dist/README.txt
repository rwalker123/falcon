ShadowScale — macOS Playtest Build
==================================

HOW TO RUN
----------
1. Unzip this folder anywhere (Desktop is fine).
2. Double-click  ShadowScale.app

   The FIRST time, macOS will refuse to open it, because this build is not signed
   by a paid Apple Developer account. To allow it:

     - Open  System Settings > Privacy & Security
     - Scroll to the bottom. There will be a line about ShadowScale being blocked.
     - Click  Open Anyway , then confirm.

   macOS remembers this, so it is a one-time step.

   (On macOS 14 and earlier you could instead right-click the app > Open > Open.
   Apple removed that shortcut in macOS 15 Sequoia, hence the settings route.)

3. The game window appears. The background simulation starts and stops on its own
   — just quit the game normally when you are done.

WHAT'S IN HERE
--------------
  ShadowScale.app   <- double-click this. The game and the simulation both live
                       inside it; there is nothing else to run.
  README.txt

NOTES
-----
* Everything runs locally on your machine — the game and the simulation talk over
  127.0.0.1 (localhost). No internet connection is needed to play.

* Nothing is installed and nothing keeps running after you quit. To uninstall,
  drag the folder to the Trash.

* If the game window opens but stays blank, give it a few seconds (it is waiting
  for the simulation's first snapshot). If it never fills, quit and reopen.

PROBLEMS?
---------
If something goes wrong the app shows an error dialog — a screenshot of that is
the most useful thing you can send back. Otherwise describe what you saw and
roughly when it happened. Thanks for playtesting!
