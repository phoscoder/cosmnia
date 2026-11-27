<p align="center"><img src="res/icons/io.github.phoscoder.Cosmnia.svg" width="128" alt="Cosmnia coffee cup"></p>
<h2 align="center">Cosmnia</h2>
<p align="center"><strong>Your COSMIC™ desktop, wide awake. One cup at a time. ☕</strong></p>
<p align="center"><em>A coffee cup in your panel: fill it and your computer never sleeps, empty it and it can rest.</em></p>

## Usage

- **Empty cup**: normal behaviour, the computer can dim, lock and suspend.
- **Click it so the cup fills**: the computer stays awake. Cosmnia holds
  - an `org.freedesktop.ScreenSaver` inhibitor, which `cosmic-idle` honours, so the screen never blanks, locks or
    suspends on idle;
  - a logind `sleep:idle` block inhibitor, so nothing can suspend the machine.
- **Click again** to empty the cup and allow sleep.

Hover the cup to see the current state. It always starts empty at login. If you quit the applet,
or it crashes, both inhibitors are released automatically.

Check it with `systemd-inhibit --list`.

## Install

Requires Rust (stable) and the usual libcosmic build deps (`libxkbcommon-dev`, `libwayland-dev`, `pkg-config`).

```sh
./install.sh            # builds and installs to ~/.local
./install.sh uninstall
```

Then: Settings → Desktop → Panel (or Dock) → Configure applets → add **Cosmnia**.
