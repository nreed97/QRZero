# Getting help

If something doesn't work or you have an idea, the quickest way to get it fixed is to open an issue on GitHub. A little information up front saves a lot of back and forth.

## Before you open an issue

1. Search this guide (the box at the top of the Help window). [Troubleshooting](22-troubleshooting) covers the common problems.
2. Check that you have the newest version: **About QRZero** in the **☰** menu shows your version, and a link in the top bar appears when a newer one is out.
3. Look at the existing issues for the same problem: **About QRZero** has a link to the issue list.

## Opening an issue

1. Open **About QRZero** from the **☰** menu and click **Report a problem or suggest a change**. This opens the project's issues page in your browser.
2. You need a free GitHub account. Click **New issue** and choose **Problem report** or **Suggestion**.
3. Fill in the form. It asks for:
   - **QRZero version**, from the About window.
   - **Operating system**, and whether you use the desktop app or a browser.
   - **What happened**: what you did, what you expected, and what happened instead. Copy any message word for word.
   - **Steps to repeat it**, if you know them.
   - **Radio and other programs**: the rig control method and radio, the version of WSJT-X, JTDX or N1MM, or the lookup service, upload service or cluster node involved.
4. Add a screenshot if it helps (drag it into the box).
5. Click **Submit new issue**.

## What not to post

- Never post passwords or API keys. QRZero keeps them in the system's password store, so they are not in your settings or backups.
- Don't attach your whole log. If one QSO matters, open it in the editor and paste its details, or export just that QSO from the log grid as ADIF and paste the line.

## Good details to include

- The exact time (UTC) it happened, if it involves a spot, a decode or an upload.
- Whether it happens every time or only sometimes.
- Whether it still happens after closing and restarting QRZero.
- For a radio problem, whether it works in another program on the same port. Remember that only one program can use a serial port at a time.
