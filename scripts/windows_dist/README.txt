ShadowScale — Windows Playtest Build
====================================

HOW TO RUN
----------
1. Unzip this folder anywhere (Desktop is fine).
2. Double-click  ShadowScale.exe

That's it. The game window appears; the background simulation starts and stops on
its own. Just close the game normally when you are done.

WHAT'S IN HERE
--------------
  ShadowScale.exe         <- double-click this
  ShadowScaleClient.exe   the game itself (started automatically)
  server.exe              the simulation (started automatically)
  shadow_scale_godot.dll  game engine plug-in (leave it next to the client)
  *.pck                   game data (leave it next to the client)

Everything must stay in the same folder.

NOTES
-----
* Everything runs locally on your machine — the game and the simulation talk over
  127.0.0.1 (localhost). No internet connection is needed to play.

* Windows SmartScreen may warn that the app is from an "unknown publisher" (these
  builds aren't code-signed). Click "More info" -> "Run anyway".

* Windows Firewall may pop up the first time asking about server.exe. It only
  listens on localhost, so you can allow it (or dismiss the prompt — local
  connections work regardless).

* Nothing is installed. To uninstall, delete the folder.

* If the game window opens but stays blank / shows no map, give it a few seconds:
  it is waiting for the simulation's first snapshot. If it never fills, close it
  and reopen ShadowScale.exe.

PROBLEMS?
---------
If something goes wrong you'll get an error dialog — a screenshot of that is the
most useful thing you can send back. Otherwise describe what you saw and roughly
when it happened. Thanks for playtesting!
