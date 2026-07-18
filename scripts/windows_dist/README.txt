ShadowScale — Windows Playtest Build
====================================

HOW TO RUN
----------
1. Unzip this folder anywhere (Desktop is fine).
2. Double-click  run.bat

That's it. A small server window opens (minimized), then the game window appears.
Closing the game also closes the server.

WHAT'S IN HERE
--------------
  run.bat                 <- double-click this
  ShadowScaleClient.exe   the game
  server.exe              the simulation (started automatically by run.bat)
  shadow_scale_godot.dll  game engine plug-in (leave it next to the client)
  *.pck                   game data (leave it next to the client)

NOTES
-----
* Everything runs locally on your machine — the two programs talk to each other
  over 127.0.0.1 (localhost). No internet connection is needed to play.

* Windows SmartScreen may warn that the app is from an "unknown publisher"
  (these builds aren't code-signed). Click "More info" -> "Run anyway".

* Windows Firewall may pop up the first time asking about server.exe. It only
  listens on localhost, so you can allow it (or dismiss the prompt — local
  connections work regardless).

* If the game window opens but stays blank / shows no map, give it a few seconds:
  the client waits for the server's first snapshot. If it never fills, close it,
  reopen run.bat, and let the server start first.

PROBLEMS?
---------
Send back:
  - what you saw (a screenshot or a description),
  - the small server window's text if it printed an error,
and roughly when it happened. Thanks for playtesting!
