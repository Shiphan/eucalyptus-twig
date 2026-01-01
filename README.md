# Eucalyptus Twig

Taskbar for Wayland (mainly Hyprland)

---

TODO:
- [ ] power menu
    - [ ] functionality
    - [ ] animation
- [ ] battery/power
    - upower (dbus): <https://upower.freedesktop.org/docs/>
    - [ ] icon with real percentage
- [x] clock
    - [x] analog clock icon
- [ ] wayland/xwayland (hyprland)
- [ ] systray
    - dbus: <https://www.freedesktop.org/wiki/Specifications/StatusNotifierItem/>
- [ ] workspaces (hyprland)
    - hyprland ipc: <https://wiki.hypr.land/IPC/>
    - maybe use this: <https://wayland.app/protocols/ext-workspace-v1>
- [ ] audio/volume
    - pipewire: <https://gitlab.freedesktop.org/pipewire/pipewire-rs>
    - pipewire-native: <https://gitlab.freedesktop.org/pipewire/pipewire-native-rs>
    - [ ] show info
    - [ ] setting panel
- [ ] internet/wifi
    - networkmanager (dbus): <https://networkmanager.dev/docs/api/latest/spec.html>
    - [ ] show info
    - [ ] setting panel
- [ ] bluetooth
    - bluez: <https://github.com/bluez/bluer>
    - [ ] show info
    - [ ] setting panel
- [ ] system info
    - [ ] cpu
    - [ ] ram
    - [ ] temperature
- [ ] power profile (power-profile-daemon)
    - [ ] show info
    - dbus: <https://upower.pages.freedesktop.org/power-profiles-daemon/gdbus-org.freedesktop.UPower.PowerProfiles.html>
    - [ ] setting panel
    - [ ] maybe also support tlp (same dbus api as ppd)
- [ ] notification
    - dbus: <https://specifications.freedesktop.org/notification/latest/>

