Bitbucket Task Bot
==================

To build (for production):

    cargo build --release

To run (in development mode):

    cargo run

To run (in development mode, with automatic reloading on code changes):

    cargo watch -x run

(Cargo watch first needs to be installed using `cargo install cargo-watch`)

To perform linting (see https://github.com/rust-lang/rust-clippy):

    cargo clippy
