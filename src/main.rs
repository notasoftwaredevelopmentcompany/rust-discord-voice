use lazy_static::__Deref;
use serenity::prelude::TypeMap;
use tokio::sync::{RwLock, RwLockWriteGuard};
use serenity::prelude::TypeMapKey;
use std::env;
use std::sync::Mutex;
use std::sync::Arc;
use std::boxed::Box;
use std::thread;

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

use songbird::{
    driver::{Config as DriverConfig, DecodeMode},
    model::payload::{ClientConnect, ClientDisconnect, Speaking},
    CoreEvent,
    Event,
    EventContext,
    EventHandler as VoiceEventHandler,
    SerenityInit,
    Songbird,
};


struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
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
                    println!("*speaking == false");
                    
                    
                    // @TODO: save the users decoded_audio into a file?
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
                    
                    let mut user_voice_data = data_lock.write().await;
                    
                    if let Some(index) = user_voice_data.iter().position(|x| x.ssrc == *ssrc) {
                        let entry = user_voice_data.get_mut(index);
                        if let Some(user_entry) = entry {
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
                        packet.ssrc,
                    ); */

                    let data_lock = {
                        // While data is a RwLock, it's recommended that you always open the lock as read.
                        // This is mainly done to avoid Deadlocks for having a possible writer waiting for multiple
                        // readers to close.
                        let data_read = self.context.data.read().await;
                
                        // Since the CommandCounter Value is wrapped in an Arc, cloning will not duplicate the
                        // data, instead the reference is cloned.
                        // We wap every value on in an Arc, as to keep the data lock open for the least time possible,
                        // to again, avoid deadlocking it.
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
        .event_handler(Handler)
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