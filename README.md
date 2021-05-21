# Discord bot with voice recording

## Host requirements
* Rust (stable)
* ffmpeg

Or just Docker, the project is setup with a Docker development container.


## Development
Create an **.env** file in the project root dir and fill out the variables.

`cargo run`

## Production
Provide the **.env** file for production and start the container.

`docker-compose up -d --build`


## Functionality
- The bot can join a predefined voice channel and listen to all users with the command:

    - `<prefix>join`

- A default serenity-rs help command is avalable:
    
    - `<prefix>help`


The bot will create an .ogg audio file every time a user speaks. The bot will then queue this file and play it back at a wrong rate of some other than 48kHz.