# Colored output like a real console

- **Date:** 2026-10-24
- **Status:** considering

## Context

The user wants RunBay output to look like the same program in a real console, without
configuring every app.

RunBay already parses ANSI SGR (§5.3, `src/ansi`): 16/256/24-bit foreground colors and
bold. Backgrounds are ignored and other escape sequences are dropped. The parser is not
what keeps output plain: RunBay reads stdout and stderr through pipes
(`src/runner/manager.rs`), and most programs (dotnet, MSBuild, cargo, npm, git, …) check
whether their output is a console and turn colors off when it is redirected. Today a
user has to force colors per tool, e.g. `DOTNET_SYSTEM_CONSOLE_ALLOW_ANSI_COLOR_REDIRECTION=1`
or `dotnet build -clp:ForceConsoleColor` (see [dotnet-colored-output](dotnet-colored-output.md)).

## Options

1. **Pseudo console (ConPTY)**: run the process tree attached to a Windows pseudo
   console (`CreatePseudoConsole`, Windows 10 1809+) and read its output pipe. Every
   program sees a real console and emits colors by itself.
   - stdout and stderr arrive as one stream; the stderr flag (§5.1) is lost.
   - ConPTY emits more than SGR: cursor movement, erase line/screen, window title,
     and repaints of progress bars. RunBay needs a terminal model (at least carriage
     return / erase line handling) or the output gets noisy.
   - A terminal width/height has to be chosen (fixed, or follow the output panel).
   - Job Object handling and Stop must keep working with the conhost/OpenConsole child.
   - Log files still store plain text.
2. **Color environment by default**: keep pipes and add color-forcing variables to
   every run (`DOTNET_SYSTEM_CONSOLE_ALLOW_ANSI_COLOR_REDIRECTION=1`, `FORCE_COLOR=1`,
   `CLICOLOR_FORCE=1`, …). Small change, stderr stays separate, but tools that ignore
   these variables (e.g. MSBuild's console logger) stay plain.
3. **Per app/action choice**: option 2 by default, with a "console mode" switch that
   uses option 1.

Independent of the option, the renderer could also support background colors
(40–47, 100–107, `48;5`, `48;2`), dim/italic/underline (2, 3, 4), and inverse (7).

## Assessment (2026-10-24)

- ConPTY gives the most faithful output, but it is a terminal, not a pipe: lines are
  wrapped at the chosen width (wrapped lines would also reach the log), tools switch
  to interactive rendering (e.g. MSBuild's terminal logger, npm spinners) that
  repaints with cursor movement, and stdout/stderr merge. Doing it well needs a
  terminal model (e.g. a VT screen crate) and more memory per run, against the
  performance targets in §15.
- The color environment covers the common cases (.NET logging, Node tools, many CLI
  tools) for a few lines of code and keeps every current behavior.
- Suggested path: option 3 in two steps. First the color environment by default
  (with a per app/action way to turn it off) plus background/dim/italic/underline/
  inverse rendering. Then, only if needed, ConPTY as an opt-in "console mode" per
  action for tools that ignore the environment.

## Decisions so far

- 2026-10-24: Go in steps (option 3).
  - **Step 1**: add color-forcing environment variables to every run by default,
    and render background colors (40–47, 100–107, `48;5`, `48;2`), dim (2),
    italic (3), underline (4), and inverse (7). Reason: covers the common tools
    with a small, low-risk change that keeps pipes, the stderr flag, unwrapped
    lines, and plain logs.
  - **Step 2**: ConPTY as an opt-in "console mode" per action, only when tools that
    ignore the environment still need it. Reason: ConPTY wraps lines, triggers
    interactive rendering, merges stdout/stderr, and costs memory, so it should not
    affect actions that do not need it.

## Open questions

Step 1 (to settle before its plan):
- The exact list of variables (e.g. `DOTNET_SYSTEM_CONSOLE_ALLOW_ANSI_COLOR_REDIRECTION`,
  `FORCE_COLOR`, `CLICOLOR_FORCE`, `PY_COLORS`, …), and whether a value set by the
  app/action always wins over RunBay's default.
- How colors are turned off: per app, per action, a global setting, or a
  combination.
- Whether MSBuild's `-clp:ForceConsoleColor` stays the script's job (documented)
  or RunBay does something for it.
- How the new attributes look in light and dark themes (e.g. background contrast,
  dim as reduced opacity).

Step 2 (only if needed):
- Terminal width and height, and whether losing the stderr flag is acceptable in
  console mode.
