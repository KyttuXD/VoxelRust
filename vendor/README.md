# vendor

Other people's libraries that ship beside the game. Nothing here is built
into the program: the game loads SDL2 when it starts.

`sdl2/` holds SDL2 as published by the SDL project (zlib licence,
https://www.libsdl.org): `SDL2.dll` for 64-bit Windows and `SDL2.framework`
(2.0.4, the last to run on Mac OS X 10.6) for the Mac apps. The folder is
not kept in the repository; `scripts/package-release.sh` and
`scripts/package-macos-app.sh` put its contents into the release files when
it is there, so that players need install nothing. On Linux the game uses
the system's SDL2 (`libsdl2-2.0-0` or your distribution's name for it).
