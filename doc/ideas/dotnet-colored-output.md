# Document how to keep .NET output colored

- **Date:** 2026-10-24
- **Status:** discussion

## Context

RunBay renders ANSI colors (§5.3), but RunBay reads output through pipes, and .NET
programs stop emitting color codes when their output is redirected. A .NET app run
from RunBay therefore shows plain output, although the same app is colored in a real
console. Two settings bring the colors back without any change to RunBay:

- `DOTNET_SYSTEM_CONSOLE_ALLOW_ANSI_COLOR_REDIRECTION=1` in the app or action
  environment: `Microsoft.Extensions.Logging` console output (e.g. ASP.NET Core
  `info`/`warn`/`fail`) keeps its ANSI colors.
- `dotnet build ... -clp:ForceConsoleColor`: MSBuild console logger keeps its colors
  (e.g. "Build succeeded", warnings, and errors).

Both were verified on 2026-10-24 with output redirected to a pipe: the logger and the
build emitted ANSI SGR sequences. The logger also sends background colors (`ESC[40m`),
which RunBay ignores by design.

## Decisions so far

- 2026-10-24: RunBay itself needs no change; this is a documentation topic.

## Open questions

- Where to document it: a "Colored output" section in the README, the specification
  (§5.3), or both?
- Should new .NET actions created from a template get these settings by default?
