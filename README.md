An experiment to mimic a fraction of `fzf`.

<p align="center">
<img src="./assets/demo.gif" width="47%" />
</p>

No crates, no `cargo`, just `rustc`.

# Quick install

```console
$ make install
```

Pressing `<C-r>` in a new terminal should just work.

> [!WARNING]
> Works only with `bash` for now

# Overview

```console
$ rush -h
USAGE: rush <MODE> [SUBMODE]
OPTS:
    MODE:
      -H [FILE]     : browse shell history from $HISTFILE or FILE
      -F <FILE>     : browse FILE
      -X <COMMAND>  : browse COMMAND output
      -             : browse piped STDIN

    SUBMODE:
      -i : insert selection to terminal input
      -p : print selection to terminal
      -o : open selection with $EDITOR

    EXPERIMENTAL:
      -C <COMMAND>  : (experimental++) compilation mode
```

# Resources

- https://viewsourcecode.org/snaptoken/kilo/02.enteringRawMode.html
- https://github.com/dcuddeback/termios-rs/
- https://github.com/mateolafalce/k_board/blob/main/src/keys.rs
- https://doc.rust-lang.org/std/collections/index.html
