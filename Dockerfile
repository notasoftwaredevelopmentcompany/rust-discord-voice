######## Builder ########
FROM rust:1.52 AS builder

# RUN rustup target add x86_64-unknown-linux-musl
RUN apt update
RUN update-ca-certificates

# Create app user
ENV USER=bob
ENV UID=10001

RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "${UID}" \
    "${USER}"


WORKDIR /rust-discord-voice

COPY ./ .

RUN cargo build --release

######## Final image ########
FROM rust:1.52-slim-buster

# Install dependencies
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get -y install --no-install-recommends ffmpeg \
    && rm -rf /var/lib/apt/lists/*

# Import from builder.
COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group

WORKDIR /rust-discord-voice

# Copy our build
COPY --from=builder /rust-discord-voice/target/release/rust-discord-voice ./

# Add bob as the owner of the dir + executable
RUN chown -R bob:bob /rust-discord-voice

# Use an unprivileged user.
USER bob:bob

CMD ["/rust-discord-voice/rust-discord-voice"]