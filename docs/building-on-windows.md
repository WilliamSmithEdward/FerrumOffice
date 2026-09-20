# Building on Windows

Two things about this toolchain cost real debugging time. Both are recorded
here so the next person does not spend it again.

## The GNU toolchain is missing an import library

`stable-x86_64-pc-windows-gnu` ships a self-contained mingw-w64 that carries
only part of the Windows API. `shlwapi` is not in it, and the interface
toolkit needs it, so the link fails with:

```text
ld: cannot find -lshlwapi
```

`apps/ferrum-grid/build.rs` fixes this by finding a full MinGW installation on
the path, copying the one missing library into the build directory, and putting
**only that directory** on the link path.

### Why not just add the MinGW lib directory

That was the first attempt, and it produced a binary that died before `main`
with `0xC00000FD`, `STATUS_STACK_OVERFLOW`, having printed nothing at all.

Adding a full MinGW `lib` directory lets the linker resolve **every** system
import library from there: `kernel32`, `user32`, `ole32` and the rest. The
binary then mixes two toolchains' idea of the Windows API, and the result is
broken in a way that surfaces during loader initialisation rather than at link
time.

The symptom is worth remembering because it is so misleading. A stack overflow
before `main` looks like runaway recursion in the interface, and two hours can
go into bisecting a layout that was never at fault. The diagnosis that settled
it was writing a file on the first line of `main` and finding the file absent:
whatever was wrong, it was not in any code this project had written.

Copying a single named library keeps everything else coming from the toolchain
doing the build.

## The MSVC toolchain avoids all of this

If `stable-x86_64-pc-windows-msvc` is installed, it links against the real
Windows SDK and neither problem arises. The build script does nothing on that
target. Use it if you have it.

## Looking at the window

Screenshots of the running application are how the interface is checked, and
two things make that awkward on this machine.

The display runs at 150%. A capture script must call
`SetProcessDpiAwareness(2)` or it works in virtualised coordinates and grabs
the wrong region: the window comes out cropped on the right and bottom, which
reads convincingly as a layout bug.

`SetForegroundWindow` is refused to a process that is not already in the
foreground, so a capture has to make the window topmost instead. Note that a
topmost, focused window then receives whatever is typed anywhere, including
into the terminal driving the capture. Two captures during this work picked up
stray characters in cell A1 and briefly looked like a data-corruption bug.

## A pointer area that fills its parent shadows its siblings

The first attempt at column resizing put a narrow grab strip over the right
edge of each column header, declared after the area that selects the column so
that it would sit on top. It never received a press. Widening the strip from
7px to 24px changed nothing, which ruled out aim and pointed at hit testing:
an area filling the header cell takes the press whatever is laid over it.

The replacement is better than a fix. Each header band now has **one** pointer
area, and whether a press means "drag this edge" or "select this column" is
decided in Rust from the pointer position, where it is nine lines and seven
tests instead of an element per column. That also removes a handler per
visible column, which would have been rebuilt on every scroll.

The hover cursor needs the same answer without a redraw, so it comes from a
`pure callback` returning a bool rather than from a property the redraw sets.

Worth remembering for the rest of the interface: prefer one pointer area per
region with the hit test in Rust, over a lattice of small areas.

## Dependency reality

The application carries one third-party dependency by name and 246 crates in
practice, which is what a GUI toolkit costs. `renderer-femtovg` was chosen over
`renderer-skia` partly for that (skia brings 291) and partly because skia's
build downloads its own source tree and needs symlink privileges Windows does
not grant by default.

Everything below the interface, which is all four library crates, has no
dependencies at all. See
[ADR 0002](adr/0002-no-runtime-dependencies.md).
