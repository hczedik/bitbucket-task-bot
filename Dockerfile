# Dockerfile for creating a statically-linked Rust application using docker's
# multi-stage build feature. This also leverages the docker build cache to avoid
# re-downloading dependencies if they have not changed.
# (inspired by https://alexbrand.dev/post/how-to-package-rust-applications-into-minimal-docker-containers/ )

FROM rust:1.47 AS build
WORKDIR /usr/src

# musl-gcc is needed by some dependencies
RUN apt-get update && apt-get install -y musl-tools

# Download the target for static linking.
RUN rustup target add x86_64-unknown-linux-musl

# Create a dummy project and build the app's dependencies.
# If the Cargo.toml or Cargo.lock files have not changed,
# we can use the docker build cache and skip these (typically slow) steps.
RUN USER=root cargo new bitbucket-task-bot
WORKDIR /usr/src/bitbucket-task-bot
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release --target x86_64-unknown-linux-musl

# Copy the source and build the application.
COPY src ./src
# necessary so that it actually re-builds
RUN touch src/main.rs
RUN cargo build --release --target x86_64-unknown-linux-musl

# Copy the statically-linked binary into a scratch container.
FROM scratch
COPY --from=build /usr/src/bitbucket-task-bot/target/x86_64-unknown-linux-musl/release/bitbucket-task-bot .
EXPOSE 8084
USER 1000
CMD ["./bitbucket-task-bot"]
