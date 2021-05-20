# Discord bot with voice recording

## Host requirements
* Rust (stable)
* ffmpeg

Or just Docker, the project is setup with a Docker development container.


## Development
`cargo run`

## Production
Todo...


## Functionality
- The bot can join a voice channel and listen to all users with the command:

    - `<prefix>join <voice_channel_id>`

- Example:

    - `?join 3129761287361238`

The bot will create an .ogg audio file every time a user speaks. The bot will then queue this file and play it back at a wrong rate of some other than 48kHz.