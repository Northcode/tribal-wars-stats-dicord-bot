#[macro_use]
extern crate serenity;
extern crate typemap;
extern crate reqwest;
extern crate select;
extern crate chrono;

#[macro_use]
extern crate custom_error;

use reqwest::Url;
use chrono::{DateTime, Utc};
use std::ops::DerefMut;
use std::ops::Deref;
use std::time::Duration;
use typemap::Key;
use serenity::prelude::*;
use serenity::model::prelude::*;
use serenity::framework::StandardFramework;
use serenity::framework::standard::help_commands;

mod scrape;
use scrape::get_and_parse_site;

macro_rules! wrap_type {
    ($name:ident, $wrapped:ty) => {
        struct $name($wrapped);

        impl Deref for $name {
            type Target = $wrapped;

            fn deref(&self) -> &$wrapped {
                return &self.0;
            }
        }

        impl DerefMut for $name {
            fn deref_mut(&mut self) -> &mut $wrapped {
                return &mut self.0;
            }
        }
    }
}

struct TestHandler;

impl EventHandler for TestHandler {}

wrap_type!(ChannelHolder, Option<ChannelId>);

impl Key for ChannelHolder {
    type Value = ChannelHolder;
}

wrap_type!(SearchList, Vec<String>);

impl Key for SearchList {
    type Value = SearchList;
}

wrap_type!(LastUpdate, DateTime<Utc>);

impl Key for LastUpdate {
    type Value = LastUpdate;
}


command!(test(_ctx, msg, _args) {
    msg.channel_id.say("Hi there!")?;
});

command!(talk_here(ctx, msg, _args) {
    println!("Talking on channel: {:?}", msg.channel_id);

    msg.channel_id.say("Ok, I'll talk on this channel")?;

    let mut data = ctx.data.lock();

    let mut channelholder = data.get_mut::<ChannelHolder>().ok_or("Failed to get channel holder")?;
    channelholder.0 = Some(msg.channel_id);
});


command!(search_for(ctx, _msg, args) {
    let mut searches = args.multiple::<String>()?;

    let mut data = ctx.data.lock();
    let searchlist = data.get_mut::<SearchList>().ok_or("Failed to get searchlist")?;

    searchlist.append(&mut searches);
});

command!(clear_searches(ctx, _msg, _args) {
    let mut data = ctx.data.lock();
    let searchlist = data.get_mut::<SearchList>().ok_or("Failed to get searchlist")?;

    searchlist.clear();
});

command!(status(ctx, msg, _args) {
    let mut data = ctx.data.lock();

    let searchlist = data.get::<SearchList>().unwrap();
    let channel = data.get::<ChannelHolder>().unwrap();
    let last_update = data.get::<LastUpdate>().unwrap();

    let resp = format!(
        r#"Currently Talking on: {:?}
Looking for events matching: {:?}
Last checked for events at: {:?}"#
            , channel.0, searchlist.0, last_update.0);

    msg.channel_id.say(resp)?;
});

const TW_SITE_URL_STR : &str = "http://de.twstats.com/de152/index.php?page=ennoblements&live=live";

fn send_new_events(events: Vec<scrape::TwEvent>, channel: ChannelId, filters: &[String], last_update: &DateTime<Utc>) {
    let msg = events.iter()
        .filter(|it|
                it.time.map(|t| t > *last_update).unwrap_or(true))
        .filter(|it| filters.iter()
                .any(|s| it.place.contains(s)
                     || it.old_holder.contains(s)
                     || it.new_holder.contains(s)))
        .map(|it|
             format!("{} has taken {} from {} at {:?}!\n", it.new_holder, it.place, it.old_holder, it.time))
        .collect::<String>();

    if msg == "" { // don't message if there were no new interesting events
        return;
    }

    let msg = format!("New events:\n{}", msg);

    if let Err(error) = channel.say(msg) {
        eprintln!("Error while writing to channel: {}", error);
    }
}

custom_error!(pub BotError
              DiscordError{source: serenity::Error} = "Discord error: {source}",
              EnvVarError{source: std::env::VarError} = "Env var error {source}",
);

fn main() -> Result<(), BotError> {

    let bot_token = "NDU5MDA5NzQyNTAxODM4ODY4.DgxISA.AKXZL-p4mge8gjcZwJIysNlWDdc";

    let bot_token = std::env::var("BOT_TOKEN").unwrap_or_else(|_| bot_token.to_string());
    let tw_site_url_str = std::env::var("BOT_TW_URL").unwrap_or_else(|_| TW_SITE_URL_STR.to_string());

    let mut discord_client = Client::new(&bot_token, TestHandler)
        .expect("Failed to create discord client");

    // Configure bot
    discord_client.with_framework(
        StandardFramework::new()
            .configure(|c| c
                       .allow_whitespace(true)
                       .on_mention(true)
                       .prefix("-")
                       .delimiters(vec![" ", ", ", ","]))
            .before(|_ctx, msg, command_name| {
                println!("Got command '{}' from '{}'", command_name, msg.author.name);
                true
            })
            .after(|_,_, command_name, error| {
                match error {
                    Ok(()) => println!("Processed command: '{}'", command_name),
                    Err(why) => eprintln!("Command '{}' returned error: {:?}", command_name, why),
                }
            })
            .customised_help(help_commands::with_embeds, |c| {
                c.individual_command_tip("Specify a command for more help.")
                    .max_levenshtein_distance(3)
            })
            .command("test", |c| c
                     .desc("Test if the bot is working, it will reply with 'Hi There!' if it is.")
                     .cmd(test))
            .command("talk_here", |c| c
                     .desc("Tell the bot to talk on this channel.")
                     .cmd(talk_here))
            .command("search_for", |c| c
                     .desc("Add a list of stuff for the bot to search for.")
                     .cmd(search_for))
            .command("clear_searches", |c| c
                     .desc("Make the bot no longer search for anything.")
                     .cmd(search_for))
            .command("status", |c| c
                     .desc("Get the status of the bot.")
                     .cmd(status))
    );

    // Initialize client data
    let client_data = discord_client.data.clone();

    {
        let mut data = client_data.lock();
        data.insert::<SearchList>(SearchList(Vec::new()));
        data.insert::<ChannelHolder>(ChannelHolder(None));
        data.insert::<LastUpdate>(LastUpdate(Utc::now()));
    }

    let tw_site_url : Url = tw_site_url_str.parse().expect("Invalid tw event url");

    // Start site polling thread
    std::thread::spawn(move || {
        loop {
            println!("polling site...");
            let events = get_and_parse_site(tw_site_url.clone());

            let mut updated = false;

            let mut data = client_data.lock();

            {
                let last_update_time = 
                {
                    let mut last_update = data.get_mut::<LastUpdate>().unwrap();
                    last_update.0
                };

                let channel = data.get::<ChannelHolder>().unwrap();
                let searches = data.get::<SearchList>().unwrap();

                match events {
                    Ok(events) => {
                        if let Some(channel) = channel.0  {
                            send_new_events(events, channel, &searches, &last_update_time);

                            updated = true;
                        }
                    },
                    Err(error) => {
                        eprintln!("Failed to fetch events: {}", error);

                        // write error to chat if talking somewhere
                        if let Some(channel) = channel.0 {

                            if let Err(err) = channel.say(format!("Failed to fetch events! Error: {}", error)) {
                                eprintln!("failed to write error msg: {}", err);
                            }
                        }
                    }
                }
            }

            if updated {
                let last_update = data.get_mut::<LastUpdate>().unwrap();
                last_update.0 = Utc::now();
            }

            drop(data); //unlock mutex

            std::thread::sleep(Duration::from_secs(30));
        }
    });

    // Run bot on main thread
    discord_client.start()?;

    Ok(())
}
