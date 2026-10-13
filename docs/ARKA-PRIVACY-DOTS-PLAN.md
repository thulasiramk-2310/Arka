# Privacy dots: plan

Status: first version built and tested in a VM, 2026-10-13. Microphone dots work
(shipped in the image, autostarts for every account). Camera and screen-share
dots are written and unit-tested but have not been seen live. The hardware layer
in the privacy daemon is not built. Not yet tried on the Nokia.

What the VM test showed: a native recorder shows "pw-cat is using the
microphone", a PulseAudio-layer recorder shows "parec says it is using the
microphone", the toast collapses to a dot after about 5 seconds, the dot goes
away when recording stops, a full-screen window does not cover it, and idle CPU
is about 0.1% of one core.

The idea: when an app is using the microphone, camera or screen, the shell
shows a dot that names the app. Phones do this; Linux desktops have no standard
equivalent (see ARKAWM-DESIGN-RESEARCH.md). It must claim only what ArkaOS can
prove, so this plan starts from what the system actually reports.

## What was measured

On a Linux laptop running PipeWire (the same audio and video system ArkaOS uses):

- When an app records, the graph gains a stream node (`Stream/Input/Audio`)
  with an active link from the microphone node (`Audio/Source`). The
  microphone node changes from `idle` to `running`.
- When the recording ends, the stream node disappears and the microphone node
  goes back to `idle`. The change is visible within a second or so.
- Cameras use the same pattern (`Video/Source`, `Stream/Input/Video`). Not
  measured yet, so the camera rows below are expected, not confirmed.
- The app name on a stream is supplied by the app and can be false. For apps
  that talk to PipeWire directly, PipeWire also records the real process ID
  (`pipewire.sec.pid`) from the connection itself, and the dot can show the
  program behind it (`/proc/<pid>/exe`). Sandboxed apps carry a portal app ID.
- Apps that use the PulseAudio compatibility layer (Firefox does) are different.
  Measured with a test recorder: `pipewire.sec.pid` is the compatibility
  server's own process ID (1448 in the test), not the recorder's (47124). The
  real ID only appears in `application.process.id`, which the app supplies. So
  for those streams the name is "reported by the app", never "verified". The
  dot itself is still reliable, because a recording stream exists.

## Two layers, so the claim stays honest

1. Who: watch PipeWire's registry. This names the app for the microphone, the
   camera through the portal, and screen sharing (a screen-share stream also
   appears in the graph).
2. Hardware truth: the privacy daemon (it runs as root) checks which processes
   hold the capture devices open (`/dev/video*`, `/dev/snd/pcm*c*`). This
   catches apps that skip PipeWire and open a device directly.

Two trust levels for the name. Verified: a native PipeWire client (process ID
from the connection) or a portal app ID. Reported: a PulseAudio-layer client, shown
as "reported by the app". What the dot may say: "Firefox is using the microphone". What it may not say:
that nothing else is. Not covered: a compromised kernel or root, and virtual
inputs (a loopback or a monitor of speaker output counts as an input).

## Pieces

- Sensor watcher: starts by parsing `pw-dump --monitor` (no new build
  dependency); moves to libpipewire if that proves too heavy.
- Daemon: exposes the current list on its existing read-only D-Bus service.
  No actions, as with the rest of the dashboard.
- Overlay: a small always-on-top shell window (layer-shell, overlay layer) at the
  top edge, so no app window or full-screen video covers it. Hover or click
  shows the app and where to change its permission.
- Colour: amber (the design system's "attention" colour), as on phones. The
  colour is a single token, easy to change.

## Done means (each in the VM first, then on the Nokia)

- A normal app recording shows a dot naming the right program, and the dot goes
  away within 2 seconds of the recording ending.
- A test app that lies about its name still shows its real program.
- Camera through the portal and screen sharing from Firefox each show a dot.
- An app that opens a capture device directly shows a dot through the second layer.
- A full-screen window does not cover it. Light and Dark both read clearly.
- Idle CPU use is near zero. Strings pass the honesty audit.

## Rough effort

First version (microphone, overlay, VM test): about 1.5 days. Camera, screen
sharing and the hardware layer: another 1 to 1.5 days.
