# Aurora: Wayland Compositor for Osmos

Aurora is a **Wayland Compositor** designed specifically for **Osmos**, an OS aimed at running the next generation of **CGUI (Conversational + Graphical UI)** applications. Aurora provides the foundational window management and graphical features needed for a modern user interface, focusing on touch support & forwarding SOUL commands to Applications.

## Current Status

- **Window Management:** Currently, you can open a compositor window using the **winit backend**. However, XDG window support, touch support, and other aspects of window management are still in progress.
- **Touch Support:** Touch events and gestures have not yet been implemented.
- **XDG Windows:** The implementation of XDG windows (including app windows, popups, and full-screen windows) is still to be completed.
- **Window Management:** Full window management, including window resizing, focus handling, and layer management, is yet to be developed.

## Features in Development

- **Touch Support**: Implementing full touch interaction support for a more immersive and intuitive experience.
- **XDG Shell Integration**: Support for XDG windows, including app window creation, resizing, and focus management.
- **Advanced Window Management**: Full window manager capabilities, including tiling, stacking, and app interaction.
- **Compositor Enhancements**: Additional compositor features for performance, animations, and user customization.

## Getting Started
### Clone the Repository

```bash
git clone https://github.com/IshantPundir/aurora.git
cd aurora
```

### Build and Run

```bash
cargo build
cargo run
```

At this point, Aurora will launch a basic compositor window. No XDG windows or advanced features are supported yet, but the foundation for the compositor is laid.

---

This README is a work in progress and will be updated as Aurora continues to evolve. Stay tuned for more features as the development progresses!

---