@help:
    just --list --unsorted

run *ARGS:
    cargo run {{ARGS}}

install:
    cargo install --path .

test:
    cargo test