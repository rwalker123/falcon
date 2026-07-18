ShadowScale — macOS Playtest Build
==================================

HOW TO RUN
----------
1. Unzip this folder anywhere (Desktop is fine).
2. IMPORTANT (first time): macOS quarantines anything downloaded from the
   internet, and this app is not signed by an Apple Developer account. Do ONE of:

   a) Open Terminal, drag the unzipped ShadowScale-macos folder onto the window
      to get its path, and run:

         xattr -dr com.apple.quarantine "/path/to/ShadowScale-macos"

      then double-click run.command.

   OR

   b) Right-click (or Control-click) run.command -> Open -> Open. Do the same for
      ShadowScaleClient.app if macOS also blocks it. After the first "Open" macOS
      remembers your choice.

3. Double-click run.command. A Terminal window opens, the server starts, then the
   game window appears. Quitting the game stops the server.

WHAT'S IN HERE
--------------
  run.command             <- double-click this
  ShadowScaleClient.app   the game (the game-engine plug-in + data are inside it)
  server                  the simulation (started automatically by run.command)
  README.txt

NOTES
-----
* Everything runs locally on your machine — the two programs talk over 127.0.0.1
  (localhost). No internet connection is needed to play.

* These builds are NOT signed or notarized, which is why macOS asks. The
  quarantine step above is the whole workaround; nothing else is needed.

* If the game window opens but stays blank, give it a few seconds (the client is
  waiting for the server's first snapshot). If it never fills, quit, and
  double-click run.command again.

PROBLEMS?
---------
Send back what you saw (a screenshot or description), any text in the Terminal
window, and roughly when it happened. Thanks for playtesting!
