use std::collections::HashSet;

use serenity::builder::{CreateEmbed, EditMessage, CreateEmbedFooter, CreateMessage};
use serenity::framework::standard::macros::{command, help};
use serenity::framework::standard::{CommandResult, help_commands, Args, HelpOptions, CommandGroup};
use serenity::model::prelude::*;
use serenity::prelude::*;
use chrono::{Duration, Utc};

use crate::utilities::global_data::{ShardManagerContainer, GuildSettingsContainer, DatabaseConnectionContainer, GuildSettings};

#[command]
#[description= "Checks Discord's API / message latency."]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    let start = Utc::now();
    let start_ts = start.timestamp();
    let start_ts_ss = start.timestamp_subsec_millis() as i64;
    let mut ping: Message = msg.channel_id.say(ctx, ":ping_pong: Pinging!").await?;
    let end = Utc::now();
    let end_ts = end.timestamp();
    let end_ts_ss = end.timestamp_subsec_millis() as i64;
    let api_response = ((end_ts - start_ts) * 1000) + (end_ts_ss - start_ts_ss);
    let ctx_data = ctx.data.read().await;
    let shard_manager = match ctx_data.get::<ShardManagerContainer>() {
        Some(shard) => shard,
        None => {
            msg.reply(ctx, "I encountered a problem while getting the shard manager.").await?;
            return Ok(());
        }
    };

    let runners = shard_manager.runners.lock().await;
    let runner = match runners.get(&ctx.shard_id) {
        Some(runner) => runner,
        None => {
            msg.reply(ctx, "Could not find a shard").await?;
            return Ok(());
        }
    };

    let shard_response = match runner.latency {
        Some(latency) => {
            if let Ok(time) = Duration::from_std(latency) {
                let time_ms = time.num_milliseconds();
                format!("`{time_ms}ms`")
            } else {
                "No latency information available".to_string()
            }
        }
        None => "No data available at the moment.".to_string()
    };

    let response = format!(
        "Pong! Succesfully retrieved the message and shard latencies. :ping_pong:\n\n\
        **API Response Time**: `{api_response}ms`\n\
        **Shard Response Time**: {shard_response}"
    );

    let embed = CreateEmbed::new().color(0x008b_0000).title("Discord Latency Information").description(response);
    ping.edit(ctx, EditMessage::new().embed(embed)).await?;

    Ok(())
}

#[command("prefix")]
//#[aliases("setprefix", "prefixset")]
#[description = "Sets the bot's guild prefix or views the current prefix."]
#[usage = "<new prefix> or leave it blank to view the current prefix."]
//#[required_permissions(ADMINISTRATOR)]
#[min_args(0)]
#[max_args(1)]
async fn prefix(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    if msg.is_private() {
        let embed = CreateEmbed::new()
            .color(0x008b_0000)
            .title("Prefix")
            .description("The bot's default prefix is ```-```")
            .footer(CreateEmbedFooter::new("Use `-setprefix <new prefix>` to change it in a server."));

        msg.channel_id.send_message(ctx, CreateMessage::new().embed(embed)).await?;

        return Ok(());
    }

    let is_admin = {
        let author = msg.guild_id.unwrap()
            .member(&ctx.http, msg.author.id).await.unwrap()
            .permissions(&ctx.cache).expect("Failed to get permissions").administrator();
        author.clone()
    };

    if !is_admin {
        let embed = CreateEmbed::new()
            .color(0x008b_0000)
            .title("Prefix")
            .description("You must be an administrator to use this command.")
            .footer(CreateEmbedFooter::new("Use `-prefix <new prefix>` to change it in a server."));

        msg.channel_id.send_message(&ctx.http, CreateMessage::new().embed(embed)).await?;

        return Ok(());
    }

    let mut arg = args.clone();
    let prefix = arg.trimmed();

    if prefix.is_empty() {
        let guild_prefix = {
            let data = ctx.data.read().await;
            let guild_settings = data.get::<GuildSettingsContainer>().unwrap();
            let pf = guild_settings.read().await;
            pf.get(&msg.guild_id.unwrap().get()).unwrap().prefix.clone()
        };

        let embed = CreateEmbed::new()
            .color(0x008b_0000)
            .title("Prefix")
            .description(format!("The bot's default prefix is ```{guild_prefix}```"))
            .footer(CreateEmbedFooter::new(format!("Use `{guild_prefix}prefix <new prefix>` to change it in a server.")));

        msg.channel_id.send_message(ctx, CreateMessage::new().embed(embed)).await?;

        return Ok(());
    }

    let set = prefix.parse::<String>().unwrap();

    if set.contains(" ") {
        let embed = CreateEmbed::new()
            .color(0x008b_0000)
            .title("Prefix")
            .description("Prefixes cannot contain spaces.");
        
        let builder = CreateMessage::new().embed(embed);

        msg.channel_id.send_message(&ctx.http, builder).await.unwrap();

        return Ok(());
    }

    let new_prefix = {

        let guild_settings = {
            let data = ctx.data.read().await;
            let guild_settings = data.get::<GuildSettingsContainer>().unwrap();

            guild_settings.clone()
        };

        let mut lock = guild_settings.write().await;

        // update guild settings
        let setting = GuildSettings {
            prefix: set.clone(),
            owner_id: msg.author.id.get(),
            mute_type: "timeout".to_string(),
            mute_role: 0
        };

        let guild_setting = lock.entry(msg.guild_id.unwrap().get()).or_insert(setting);
        guild_setting.prefix = set;

        guild_setting.prefix.clone()
    };

    {
        let data = ctx.data.read().await;
        let database = data.get::<DatabaseConnectionContainer>().unwrap().clone();
        let guild_id = msg.guild_id.unwrap().get() as i64;

        sqlx::query!(
            "UPDATE guild_settings SET prefix = ? WHERE guild_id = ?",
            new_prefix,
            guild_id
        ).execute(&database).await.unwrap();
    }

    let embed = CreateEmbed::new()
        .color(0x008b_0000)
        .title("Prefix")
        .description(format!("Prefix set to ```{new_prefix}```"));

    msg.channel_id.send_message(ctx, CreateMessage::new().embed(embed)).await?;

    Ok(())
}

#[help]
#[max_levenshtein_distance(3)]
#[indention_prefix = "+"]
#[lacking_permissions = "Hide"]
#[lacking_role = "Nothing"]
#[wrong_channel = "Strike"]
#[no_help_available_text("No help information available.")]
#[individual_command_tip = "Hello! こんにちは！Hola! Bonjour! 您好! 안녕하세요~\n\n\
If you want more information about a specific command, just pass the command as argument."]
async fn help(ctx: &Context, msg: &Message, args: Args, opts: &'static HelpOptions, groups: &[&'static CommandGroup], owners: HashSet<UserId>) -> CommandResult {
    let _ = help_commands::with_embeds(ctx, msg, args, opts, groups, owners).await;
    Ok(())
}