pub mod event_handler {
    use std::sync::Arc;
    use std::sync::atomic::{Ordering, AtomicBool};
    use std::time::Duration;

    use serenity::async_trait;
    use serenity::builder::CreateAllowedMentions;
    use serenity::client::EventHandler;
    use serenity::gateway::ActivityData;
    use serenity::model::channel::Message;
    use serenity::model::gateway::Ready;
    use serenity::all::{Context, ResumedEvent, Guild, UnavailableGuild, GuildChannel, GuildId};
    use tracing::info;

    use crate::utilities::global_data::{DatabaseConnectionContainer, GuildSettingsContainer, GuildSettings};
    pub struct Handler {
        pub database: sqlx::SqlitePool,
        pub is_loop_running: AtomicBool,
    }

    #[async_trait]
    impl EventHandler for Handler {
        // Set a handler for the `message` event - so that whenever a new message is received - the
        // closure (or function) passed will be called.
        //
        // Event handlers are dispatched through a threadpool, and so multiple events can be dispatched
        // simultaneously.
        async fn message(&self, _ctx: Context, msg: Message) {
            // TODO: add advanced command handler + database connection

            // ignore all bots, including the bot itself
            if msg.author.bot {
                return;
            }

            // trim the end to make it easier for mobile users
            let content = msg.content.trim_end();

            if content == "<@!1183487567094632638>" || content == "<@1183487567094632638>" {
                let prefix = {
                    let data = _ctx.data.read().await;
                    let guild_settings = data.get::<GuildSettingsContainer>().unwrap();
                    let pf = guild_settings.read().await;
                    pf.get(&msg.guild_id.unwrap().get()).unwrap().prefix.clone()
                };

                let embed = serenity::builder::CreateEmbed::new()
                .title("**Hello!**")
                .description(format!("```To see the list of commands type {}help```", prefix));

                let builder = serenity::builder::CreateMessage::new()
                .add_embed(embed)
                .allowed_mentions(CreateAllowedMentions::new().users(vec![msg.author.id]))
                .reference_message(&msg);

                msg.channel_id.send_message(&_ctx, builder).await.unwrap();
            }
        }

        async fn thread_create(&self, ctx: Context, thread: GuildChannel) {
            if let Err(err) = thread.id.join_thread(ctx.http).await {
                let thread_id = thread.id;
                info!("Failed to succesfully join thread (ID: {thread_id}): {err}")
            } else {
                let name = &thread.name;
                let guild = &thread.guild(&ctx.cache).unwrap().name;
                let id = thread.id.get();
                info!("Joined new thread: {name} (Server: {guild}, ID: {id})")
            }
        }

        async fn guild_create(&self, ctx: Context, guild: Guild, _: Option<bool>) {
            // write into database and hashmap
            info!("Connected to guild: {}", guild.name);
            info!("Guild ID: {}", guild.id);
            info!("Guild Owner ID: {}", guild.owner_id);
            info!("Guild Members: {}", guild.member_count);

            let data = ctx.data.read().await;
            let database = data.get::<DatabaseConnectionContainer>().unwrap().clone();
            let (guild_id, owner_id) = {
                let guild_id = i64::from(guild.id);
                let owner_id = i64::from(guild.owner_id);

                (guild_id, owner_id)
            };

            sqlx::query!(
                "INSERT INTO guild_settings (
                    guild_id,
                    prefix,
                    owner_id
                ) VALUES (?, ?, ?) ON CONFLICT DO NOTHING",
                guild_id,
                "-",
                owner_id
            ).execute(&database).await.unwrap();

            let owner_id_u64 = owner_id as u64;
            let guild_id_u64 = guild_id as u64;

            let data_to_set = GuildSettings {
                prefix: "-".to_string(),
                owner_id: owner_id_u64,
                mute_type: "timeout".to_string(),
                mute_role: 0
            };

            {
                let mut guild_settings = data.get::<GuildSettingsContainer>().unwrap().write().await;
                guild_settings.insert(guild_id_u64, data_to_set);
            }

            info!("Guild settings set complete for guild {}", guild.name);
        }

        async fn guild_delete(&self, ctx: Context, _: UnavailableGuild, g: Option<Guild>) {
            let guild = g.unwrap();
            info!("Left guild: {}", guild.name);
            // write into database and hashmap
            {
                let data = ctx.data.read().await;
                let database = data.get::<DatabaseConnectionContainer>().unwrap().clone();
                let guild_id = i64::from(guild.id);
                sqlx::query!(
                    "DELETE FROM guild_settings WHERE guild_id = ?",
                    guild_id
                ).execute(&database).await.unwrap();
            }
        }

        // Set a handler to be called on the `ready` event. This is called when a shard is booted, and
        // a READY payload is sent by Discord. This payload contains data like the current user's guild
        // Ids, current user data, private channels, and more.
        //
        // In this case, just print what the current user's username is.
        async fn ready(&self, context: Context, ready: Ready) {
            let http = &context.http;

            let api_version = ready.version;
            let bot_gateway = http.get_bot_gateway().await.unwrap();
            let bot_owner = http.get_current_application_info().await.unwrap().owner.expect("Couldn't get bot owner");
            let t_sessions = bot_gateway.session_start_limit.total;
            let r_sessions = bot_gateway.session_start_limit.remaining;
            let shard_info = ready.shard.unwrap();

            info!("Successfully logged into Discord as the following user:");
            info!("Bot username: {}", ready.user.tag());
            info!("Bot user ID: {}", ready.user.id);
            info!("Bot owner: {}", bot_owner.tag());

            let guild_count = ready.guilds.len();

            info!("Connected to shard {} out of a total of {} shards.", shard_info.id, shard_info.total);
            info!("Connected to the Discord API (version {api_version}) with {r_sessions}/{t_sessions} sessions remaining.");
            info!("Connected to and serving a total of {guild_count} guild(s).");
        }

        async fn cache_ready(&self, ctx: Context, guilds: Vec<GuildId>) {
            println!("Cache built successfully!");
    
            // It's safe to clone Context, but Arc is cheaper for this use case.
            // Untested claim, just theoretically. :P
            let ctx = Arc::new(ctx);
    
            // We need to check that the loop is not already running when this event triggers, as this
            // event triggers every time the bot enters or leaves a guild, along every time the ready
            // shard event triggers.
            //
            // An AtomicBool is used because it doesn't require a mutable reference to be changed, as
            // we don't have one due to self being an immutable reference.
            if !self.is_loop_running.load(Ordering::Relaxed) {
    
                // And of course, we can run more than one thread at different timings.
                let ctx2 = Arc::clone(&ctx);
                tokio::spawn(async move {
                    loop {
                        set_activity(&ctx2, guilds.len());
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }
                });
    
                // Now that the loop is running, we set the bool to true
                self.is_loop_running.swap(true, Ordering::Relaxed);
            }
        }

        async fn resume(&self, _: Context, _: ResumedEvent) {
            info!("Resumed!");
        }
    }

    fn set_activity(ctx: &Context, guild_count: usize) {
        let presence = format!("Monitoring a total of {guild_count} guilds | -help");
        
        ctx.set_activity(Some(ActivityData::playing(presence)));
    }
    
}