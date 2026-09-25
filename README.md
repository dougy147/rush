This is an experiment to reproduce a fraction of fzf.
I chose Rust to force myself programming in this syntax-noisy language.

# Quick start

```console
$ source ./rush.bash # then press <C-r>
```

# Compile

```console
$ rustc rush.rs
```

Compiling with optimization could improve speed on opening, which is especially good in HISTORY mode.
However, some unexplained bugs can occur with it, so use it only when feeling debugging

```console
$ rustc -C opt-level=1 rush.rs
```

# SOURCES

- https://viewsourcecode.org/snaptoken/kilo/02.enteringRawMode.html
- https://github.com/dcuddeback/termios-rs/
- https://github.com/mateolafalce/k_board/blob/main/src/keys.rs
- https://doc.rust-lang.org/std/collections/index.htmlrustc -C opt-level=1 rush.rs
