# hyprsca
A screen configuration assistant for Hyprland

## Features
 - can save current screen configuration and easily restore it later
 - does not require to run as daemon in the background

## Objective
There are already a number of programs available that can automatically restore
screen configuration when screens or connected or disconnected. Unlike those,
hyprsca does not wait for screens to be connected or disconnected. Instead, the
user must manually invoke it to restore a saved screen configuration. This can
be done easily by adding a key binding to Hyprland. Why? Because the user
should be able to do manual changes to the screen config when they so choose,
using either command line tools (like hyprctl), or graphical tools (like
wdisplays). The user stays in control. With a single keystroke they can restore
the saved configuration - but only if and when they wish.

The current screen configuration can be saved with `hyprsca save`. The saved
configuration for the current set of connected outputs (by checking make,
model, serial number) can be restored with `hyprsca restore`. The latter can be
bound to e.g. SUPER+O:
```
bindl = SUPER, O, exec, hyprsca restore
```
`bindl` is used, so that this shortcut works even on the lock screen.

## Planned Features
The first version of hyprsca is extremely simple and implemented in under 200
lines of Rust. The next feature I want to implement is a way to ignore the
builtin laptop screen depending on the state of the laptop lid. This way, you
can have separate configurations for opened and closed laptop lid.
