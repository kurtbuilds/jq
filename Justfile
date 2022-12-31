set positional-arguments

@help:
    just --list --unsorted

run *ARGS:
    cargo run -- "$@"

install:
    cargo install --path .

test:
    cargo test