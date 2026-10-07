# Using QRZero in a browser

The desktop app is a window onto a small web server built into QRZero. You can also run that server on its own and use a browser instead:

```
qrzero-server --data-dir "C:\path\to\data" --bind 127.0.0.1:8073
```

It prints a link including a session token. Open that link; the token keeps other programs on the computer from reading your log.

The server only listens on this computer (`127.0.0.1`) unless you tell it otherwise. Don't bind it to your network until the planned network sign-in is in place.
