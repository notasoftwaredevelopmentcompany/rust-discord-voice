FROM rust:1.51

RUN apt-get update && apt-get install -y ffmpeg && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/rust-discord-voice
COPY . .

RUN cargo install --path .

CMD ["rust-discord-voice"]