use tokio::sync::{RwLock};
use serenity::{model::id::GuildId, prelude::TypeMapKey};
use std::{env, fs::{self, File}, io, process::Command, sync::atomic::AtomicBool, sync::atomic::Ordering, time::Duration};
use std::sync::Arc;
use std::boxed::Box;
use std::thread;
use std::io::{Write};
use uuid::Uuid;

use serenity::{
    async_trait,
    client::{Client, Context, EventHandler},
    framework::{
        StandardFramework,
        standard::{
            macros::{command, group},
            Args, CommandResult,
        },
    },
    model::{
        channel::Message,
        gateway::Ready,
        id::ChannelId,
        misc::Mentionable
    },
    Result as SerenityResult,
};

use songbird::{CoreEvent, Event, EventContext, EventHandler as VoiceEventHandler, SerenityInit, Songbird, driver::{Config as DriverConfig, DecodeMode}, ffmpeg, model::payload::{ClientConnect, ClientDisconnect, Speaking}};


struct Handler {
    is_loop_running: AtomicBool,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }

    // We use the cache_ready event just in case some cache operation is required in whatever use
    // case you have for this.
    async fn cache_ready(&self, ctx: Context, _guilds: Vec<GuildId>) {
        println!("Cache built successfully!");

        // it's safe to clone Context, but Arc is cheaper for this use case.
        // Untested claim, just theoretically. :P
        let ctx = Arc::new(ctx);

        // We need to check that the loop is not already running when this event triggers,
        // as this event triggers every time the bot enters or leaves a guild, along every time the
        // ready shard event triggers.
        //
        // An AtomicBool is used because it doesn't require a mutable reference to be changed, as
        // we don't have one due to self being an immutable reference.
        if !self.is_loop_running.load(Ordering::Relaxed) {

            // We have to clone the Arc, as it gets moved into the new thread.
            let ctx1 = Arc::clone(&ctx);
            // tokio::spawn creates a new green thread that can run in parallel with the rest of
            // the application.
            tokio::spawn(async move {
                loop {
                    // We clone Context again here, because Arc is owned, so it moves to the
                    // new function.
                    // log_system_load(Arc::clone(&ctx1)).await;
                    // tokio::time::sleep(Duration::from_secs(120)).await;

                    let manager = songbird::get(&ctx1).await
                        .expect("Songbird Voice client placed in at initialisation.").clone();

                    let guild_id = GuildId(339824391007502344);

                    if let Some(handler_lock) = manager.get(guild_id) {
                        let mut handler = handler_lock.lock().await;

                        let audio_source = match ffmpeg("./ogg_bla_filetype_test_audio.ogg").await {
                            Ok(audio_source) => audio_source,
                            Err(why) => {
                                println!("Err starting source: {:?}", why);
                                return ();
                            },
                        };

                        handler.play_source(audio_source);
                    }

                    tokio::time::sleep(Duration::from_secs(1)).await;   
                }
            });

            // And of course, we can run more than one thread at different timings.
            /* let ctx2 = Arc::clone(&ctx);
            tokio::spawn(async move {
                loop {
                    // perform work & sleep
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
            }); */

            // Now that the loop is running, we set the bool to true
            self.is_loop_running.swap(true, Ordering::Relaxed);
        }
    }
}


struct Receiver {
    // user_voice_data: &'a mut Vec<UserVoiceData>,
    // call_lock: Weak<Mutex<Call>>,
    context: Context,
}

#[derive(Debug)]
#[derive(Clone)]
struct UserVoiceData {
    ssrc: u32,
    decoded_audio: Vec<i16>
}


// A container type is created for inserting into the Client's `data`, which
// allows for data to be accessible across all events and framework commands, or
// anywhere else that has a copy of the `data` Arc.
// These places are usually where either Context or Client is present.
//
// Documentation about TypeMap can be found here:
// https://docs.rs/typemap_rev/0.1/typemap_rev/struct.TypeMap.html
struct UserVoiceDataVector;

impl TypeMapKey for UserVoiceDataVector {
    type Value = Arc<RwLock<Vec<UserVoiceData>>>;
}



#[async_trait]
impl VoiceEventHandler for Receiver {
    #[allow(unused_variables)]
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        use EventContext as Ctx;

        match ctx {
            Ctx::SpeakingStateUpdate(
                Speaking {speaking, ssrc, user_id, ..}
            ) => {
                // Discord voice calls use RTP, where every sender uses a randomly allocated
                // *Synchronisation Source* (SSRC) to allow receivers to tell which audioUserVoiceData
                // stream a received packet belongs to. As this number is not derived from
                // the sender's user_id, only Discord Voice Gateway messages like this one
                // inform us about which random SSRC a user has been allocated. Future voice
                // packets will contain *only* the SSRC.
                //
                // You can implement logic here so that you can differentiate users'
                // SSRCs and map the SSRC to the User ID and maintain this state.
                // Using this map, you can map the `ssrc` in `voice_packet`
                // to the user ID and handle their audio packets separately.
                println!(
                    "Speaking state update: user {:?} has SSRC {:?}, using {:?}",
                    user_id,
                    ssrc,
                    speaking,
                );
            },
            Ctx::SpeakingUpdate {ssrc, speaking} => {
                // You can implement logic here which reacts to a user starting
                // or stopping speaking.
                println!(
                    "Source {} has {} speaking.",
                    ssrc,
                    if *speaking {"started"} else {"stopped"},
                );

                if *speaking == true {
                    let new_user_user_voice_data = UserVoiceData {
                        ssrc: *ssrc,
                        decoded_audio: Vec::<i16>::new(),
                    };
                    
                    let data_lock = {
                        // While data is a RwLock, it's recommended that you always open the lock as read.
                        // This is mainly done to avoid Deadlocks for having a possible writer waiting for multiple
                        // readers to close.
                        let data_read = self.context.data.read().await;
                
                        // Since the UserVoiceDataVector Value is wrapped in an Arc, cloning will not duplicate the
                        // data, instead the reference is cloned.
                        // We wap every value on in an Arc, as to keep the data lock open for the least time possible,
                        // to again, avoid deadlocking it.
                        data_read.get::<UserVoiceDataVector>().expect("Expected UserVoiceDataVector in TypeMap.").clone()
                    };
                
                    // Just like with client.data in main, we want to keep write locks open the least time
                    // possible, so we wrap them on a block so they get automatically closed at the end.
                    {
                        // The HashMap of CommandCounter is wrapped in an RwLock; since we want to write to it, we will
                        // open the lock in write mode.
                        let mut user_voice_data = data_lock.write().await;
                        user_voice_data.push(new_user_user_voice_data);
                    }
                } else {
                    let data_lock = {
                        let data_read = self.context.data.read().await;
                        data_read.get::<UserVoiceDataVector>().expect("Expected UserVoiceDataVector in TypeMap.").clone()    
                    };
                    
                    let mut user_voice_data = data_lock.write().await;
                    
                    if let Some(index) = user_voice_data.iter().position(|x| x.ssrc == *ssrc) {
                        let entry = user_voice_data.get_mut(index);
                        if let Some(user_entry) = entry {                            
                            let decoded_audio = user_entry.decoded_audio.clone();
                            
                            thread::spawn(move || {
                                let uuid = format!("{}", Uuid::new_v4());
                                let raw_file_name = format!("output_{}.opus", uuid);
                                let output_file_name = format!("ogg_{}.ogg", uuid);
                                
                                {
                                    let mut raw_output_file = File::create(raw_file_name).expect("Unable to create opus file");
    
                                    println!("[New thread]: writing decoded_audio to file");
                                    for i in decoded_audio { 
                                        let result = raw_output_file.write_all(&i.to_le_bytes());
                                        match result {
                                            Ok(v) => (),
                                            Err(e) => println!("[New thread]: Error writing audio to file: {:?}", e),
                                        }
                                    }

                                    println!("[New thread]: File written successfully!");
                                } // @NOTE: the raw_output_file (write only) file handle gets close at the end of this scope

                                // Encoding the opus output file into ogg
                                // Example: ffmpeg -f s16le -ar 48k -ac 2 -i output.opus output_test.ogg
                                let output_of_ffmpeg = Command::new("ffmpeg")
                                    .arg("-f")
                                    .arg("s16le")
                                    .arg("-ar")
                                    .arg("48k")
                                    .arg("-ac")
                                    .arg("2")
                                    .arg("-i")
                                    .arg(format!("output_{}.opus", uuid))
                                    .arg(output_file_name)
                                    .output()
                                    .expect("failed to execute process");

                                // println!("status: {}", output_of_ffmpeg.status);
                                // io::stdout().write_all(&output_of_ffmpeg.stdout).unwrap();
                                io::stderr().write_all(&output_of_ffmpeg.stderr).unwrap();

                                // Delete the original raw opus file from disk
                                let remove_file_result = fs::remove_file(format!("output_{}.opus", uuid));
                                match remove_file_result {
                                    Ok(v) => println!("The raw opus file: {} was successfully removed", format!("output_{}.opus", uuid)),
                                    Err(e) => println!("Unable to remove the raw opus file: {}", format!("output_{}.opus", uuid)),
                                }
                            });
                            
                            // Reset the users decoded_audio
                            let mut decoded_audio_length: usize = user_entry.decoded_audio.len();
                            println!("Their decoded_audio length was: {}", decoded_audio_length);
                            user_entry.decoded_audio = Vec::new();

                            decoded_audio_length = user_entry.decoded_audio.len();
                            println!("Their decoded_audio was reset to: {}", decoded_audio_length);
                        }
                    }
                }
            },
            Ctx::VoicePacket {audio, packet, payload_offset, payload_end_pad} => {
                // An event which fires for every received audio packet,
                // containing the decoded data.
                if let Some(audio) = audio {
                    /* println!("Audio packet's first 5 samples: {:?}", audio.get(..5.min(audio.len())));
                    println!(
                        "Audio packet sequence {:05} has {:04} bytes (decompressed from {}), SSRC {}",
                        packet.sequence.0,
                        audio.len() * std::mem::size_of::<i16>(),
                        packet.payload.len(),
                        packet.ssrc
                    ); */

                    let data_lock = {
                        let data_read = self.context.data.read().await;
                        data_read.get::<UserVoiceDataVector>().expect("Expected UserVoiceDataVector in TypeMap.").clone()
                    };
                
                    {
                        let mut user_voice_data = data_lock.write().await;

                        if let Some(index) = user_voice_data.iter().position(|x| x.ssrc == packet.ssrc) {
                            let entry = user_voice_data.get_mut(index);
    
                            if let Some(user_entry) = entry {
                                for sample in audio {
                                    user_entry.decoded_audio.push(*sample);
                                }
                            }
                        }
                    }

                } else {
                    println!("RTP packet, but no audio. Driver may not be configured to decode.");
                }
            },
            Ctx::RtcpPacket {packet, payload_offset, payload_end_pad} => {
                // An event which fires for every received rtcp packet,
                // containing the call statistics and reporting information.
                // println!("RTCP packet received: {:?}", packet);
            },
            Ctx::ClientConnect(
                ClientConnect {audio_ssrc, video_ssrc, user_id, ..}
            ) => {
                // You can implement your own logic here to handle a user who has joined the
                // voice channel e.g., allocate structures, map their SSRC to User ID.

                println!(
                    "Client connected: user {:?} has audio SSRC {:?}, video SSRC {:?}",
                    user_id,
                    audio_ssrc,
                    video_ssrc,
                );
            },
            Ctx::ClientDisconnect(
                ClientDisconnect {user_id, ..}
            ) => {
                // You can implement your own logic here to handle a user who has left the
                // voice channel e.g., finalise processing of statistics etc.
                // You will typically need to map the User ID to their SSRC; observed when
                // speaking or connecting.

                println!("Client disconnected: user {:?}", user_id);
            },
            Ctx::Track(_) => {}
            Ctx::DriverConnect => {}
            Ctx::DriverReconnect => {}
            Ctx::DriverConnectFailed => {}
            Ctx::DriverReconnectFailed => {}
            Ctx::SsrcKnown(_) => {}
            _ => println!("something else"),
        }

        None
    }
}

#[group]
#[commands(join, leave, ping)]
struct General;


#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let prefix = env::var("PREFIX").expect("Expected a PREFIX in the environment");

    let framework = StandardFramework::new()
        .configure(|c| c.prefix(&prefix))
        .group(&GENERAL_GROUP);
    
    // Here, we need to configure Songbird to decode all incoming voice packets.
    // If you want, you can do this on a per-call basis---here, we need it to
    // read the audio data that other people are sending us!
    let songbird = Songbird::serenity();
    songbird.set_config(
        DriverConfig::default()
            .decode_mode(DecodeMode::Decode)
    );

    // Login with a bot token from the environment
    let token = env::var("DISCORD_TOKEN").expect("Expected a DISCORD_TOKEN in the environment");
    let mut client = Client::builder(token)
        .event_handler(Handler {
            is_loop_running: AtomicBool::new(false),
        })
        .framework(framework)
        .register_songbird_with(songbird.into())
        .await
        .expect("Error creating client");
    
    // This is where we can initially insert the data we desire into the "global" data TypeMap.
    // client.data is wrapped on a RwLock, and since we want to insert to it, we have to open it in
    // write mode, but there's a small thing catch:
    // There can only be a single writer to a given lock open in the entire application, this means
    // you can't open a new write lock until the previous write lock has closed.
    // This is not the case with read locks, read locks can be open indefinitely, BUT as soon as
    // you need to open the lock in write mode, all the read locks must be closed.
    //
    // You can find more information about deadlocks in the Rust Book, ch16-03:
    // https://doc.rust-lang.org/book/ch16-03-shared-state.html
    //
    // All of this means that we have to keep locks open for the least time possible, so we put
    // them inside a block, so they get closed automatically when droped.
    // If we don't do this, we would never be able to open the data lock anywhere else.
    {
        // Open the data lock in write mode, so keys can be inserted to it.
        let mut data = client.data.write().await;
        data.insert::<UserVoiceDataVector>(Arc::new(RwLock::new(Vec::<UserVoiceData>::new())));
    }

    // start listening for events by starting a single shard
    if let Err(why) = client.start().await {
        println!("An error occurred while running the client: {:?}", why);
    }
}

#[command]
#[only_in(guilds)]
async fn join(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let connect_to = match args.single::<u64>() {
        Ok(id) => ChannelId(id),
        Err(_) => {
            check_msg(msg.reply(ctx, "Requires a valid voice channel ID be given").await);

            return Ok(());
        },
    };

    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialisation.").clone();

    let (handler_lock, conn_result) = manager.join(guild_id, connect_to).await;


    if let Ok(_) = conn_result {
        // NOTE: this skips listening for the actual connection result.
        let mut handler = handler_lock.lock().await;

        let context_clone = ctx.clone();
        handler.add_global_event(
            CoreEvent::SpeakingStateUpdate.into(),
            Receiver {
                context: context_clone
            }
        );

        let context_clone = ctx.clone();
        handler.add_global_event(
            CoreEvent::SpeakingUpdate.into(),
            Receiver {
                context: context_clone
            }
        );

        let context_clone = ctx.clone();
        handler.add_global_event(
            CoreEvent::VoicePacket.into(),
            Receiver {
                context: context_clone
            }
        );

        let context_clone = ctx.clone();
        handler.add_global_event(
            CoreEvent::RtcpPacket.into(),
            Receiver {
                context: context_clone
            }
        );

        let context_clone = ctx.clone();
        handler.add_global_event(
            CoreEvent::ClientConnect.into(),
            Receiver {
                context: context_clone
            }
        );

        let context_clone = ctx.clone();
        handler.add_global_event(
            CoreEvent::ClientDisconnect.into(),
            Receiver {
                context: context_clone
            }
        );

        check_msg(msg.channel_id.say(&ctx.http, &format!("Joined {}", connect_to.mention())).await);
    } else {
        check_msg(msg.channel_id.say(&ctx.http, "Error joining the channel").await);
    }

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn leave(ctx: &Context, msg: &Message) -> CommandResult {
    let guild = msg.guild(&ctx.cache).await.unwrap();
    let guild_id = guild.id;

    let manager = songbird::get(ctx).await
        .expect("Songbird Voice client placed in at initialisation.").clone();
    let has_handler = manager.get(guild_id).is_some();

    if has_handler {
        if let Err(e) = manager.remove(guild_id).await {
            check_msg(msg.channel_id.say(&ctx.http, format!("Failed: {:?}", e)).await);
        }

        check_msg(msg.channel_id.say(&ctx.http,"Left voice channel").await);
    } else {
        check_msg(msg.reply(ctx, "Not in a voice channel").await);
    }

    Ok(())
}

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    check_msg(msg.channel_id.say(&ctx.http,"Pong!").await);

    Ok(())
}

/// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg(result: SerenityResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}