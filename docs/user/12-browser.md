# Using QRZero in a browser

The desktop app is a window onto a small web server built into QRZero. You can also run that server on its own and use a browser instead. It is a separate download, not part of the installer: from the release page get `QRZero-server_<version>_windows-x86_64.exe` (Linux: `QRZero-server_<version>_linux-x86_64.tar.gz`) and save it somewhere, for example `C:\QRZero\qrzero-server.exe`. The installer and portable app do not put `qrzero-server` on your PATH, so in PowerShell give the full path:

```
C:\QRZero\qrzero-server.exe --data-dir "C:\path\to\data" --bind 127.0.0.1:8073
```

Point `--data-dir` at the folder holding your log (the desktop app keeps it in `%APPDATA%\QRZero`). Close the desktop app first, because both would use the same log and the same UDP ports.

It prints a link including a session token. Open that link; the token keeps other programs on the computer from reading your log.

The server only listens on this computer (`127.0.0.1`) unless you tell it otherwise. Don't bind it to your network until the planned network sign-in is in place.
