# RunBay

A small, fast Windows tool to build and run your applications from one place.

Register your apps, give each one Build / Run / Build+Run and custom actions
backed by PowerShell scripts, and watch their output live — no more piles of
console windows and desktop shortcuts. RunBay works as a full window, a compact
list, or a floating icon that stays on top.

> **Status:** specification stage — no release yet. See
> [doc/specification.md](doc/specification.md).

## Planned features

- Apps with icons, groups, working directory, and environment variables.
- Build, Run, Build+Run, and custom actions as PowerShell scripts
  (`pwsh` 7, falling back to Windows PowerShell 5.1).
- Live, colored output and a per-action run history with configurable retention.
- Three display modes: Full, Compact, and a draggable floating icon.
- Always on top, system tray, load with Windows, and remembered window positions.
- Material 3 Expressive design, built with Rust and [Slint](https://slint.dev).

## License

[MIT](LICENSE)
