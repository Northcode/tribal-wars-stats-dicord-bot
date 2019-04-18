extern crate select;
extern crate chrono;
extern crate hyper;
extern crate serenity;
extern crate typemap;
extern crate regex;

use select::document::Document;
use select::predicate::{Attr, Name};
use select::node::Node;

use chrono::DateTime;
use chrono::prelude::*;

use hyper::rt::{self, lazy, Future, Stream};

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serenity::prelude::*;
use serenity::model::channel::*;
use serenity::model::id::ChannelId;
use typemap::Key;

use chrono::ParseError;
use std::num::ParseIntError;

use std::collections::HashMap;

use regex::Regex;

use std::fmt::Write;

/// TribalWars Event
#[derive(Debug)]
struct TwEvent {
    place: String,
    points: i32,
    old_holder: String,
    new_holder: String,
    time: DateTime<Utc>,
}

#[derive(Debug)]
enum TwEventParseError {
    NotEnoughColumns(ValueError),
    DateParse(ParseError),
    PointsParse(ParseIntError)
}

impl From<ValueError> for TwEventParseError {
    fn from(error: ValueError) -> Self {
        TwEventParseError::NotEnoughColumns(error)
    }
}

impl From<ParseError> for TwEventParseError {
    fn from(error: ParseError) -> Self {
        TwEventParseError::DateParse(error)
    }
}

impl From<ParseIntError> for TwEventParseError {
    fn from(error: ParseIntError) -> Self {
        TwEventParseError::PointsParse(error)
    }
}

#[derive(Debug)]
struct ValueError {
    msg: String
}

fn try<A>(o: Option<A>) -> Result<A, ValueError> {
    match o {
        Some(v) => Ok(v),
        None => Err(ValueError { msg: "No value in option!".to_string() })
    }
}

fn parse_row(row: Node<'_>) -> Result<TwEvent, TwEventParseError> {

    let mut itr = row.find(Name("td"));

    let place : String = try(itr.next())?.text();
    let point_str : String = str::replace(try(itr.next())?.text().as_ref(), ",", "");
    let old_holder = try(itr.next())?.text();
    let new_holder = try(itr.next())?.text();
    let time_str = try(itr.next())?.text();

    let time = Utc.datetime_from_str(time_str.as_str(), "%Y-%m-%d - %H:%M:%S")?;
    let points : i32 = point_str.parse()?;

    Ok(TwEvent { place, points, old_holder, new_holder, time })
}

fn parse_doc(docstr: &str) -> Vec<TwEvent> {
    let mut coll = vec![];

    let document = Document::from(docstr);
    
    for table in document.find(Attr("class","widget")) {
        for row in table.find(Name("tr")) {
            let twevent = parse_row(row);

            if let Ok(twevent) = twevent {
                coll.push(twevent);
            }
        }
    }

    coll
}

struct ChannelHolder;

impl Key for ChannelHolder {
    type Value = Arc<Mutex<Option<ChannelId>>>;
}

struct SubsHolder {
    filter_names: Vec<String>
}

impl SubsHolder {
    fn new() -> SubsHolder {
        SubsHolder { filter_names: Vec::new() }
    }
}

impl Key for SubsHolder {
    type Value = Arc<Mutex<SubsHolder>>;
}

struct BotHandler;

impl EventHandler for BotHandler {

    fn message(&self, ctx: Context, msg: Message) {
        println!("Got message: {:?}", msg);

        let search_for_re = Regex::new(r"-search_for\s+(.*)").unwrap();

        if !msg.author.bot {
            if msg.content == "-test" {
                msg.channel_id.say("Hi there!").expect("Failed to communicate with discord, something is wrong!");
            } else if msg.content == "-talk_here" {
                println!("Talking on channel: {:?}", msg.channel_id);
                msg.channel_id.say("Ok. I'll talk on this channel").expect("Failed to communicate with discord, something is wrong!");

                let mut data = ctx.data.lock();

                let channelholder = data.get_mut::<ChannelHolder>().unwrap();
                let mut guard = channelholder.lock().unwrap();
                *guard = Some(msg.channel_id);
            } else if let Some(captures) = search_for_re.captures(msg.content.as_ref()) {
                let search_str = captures.get(1).unwrap().as_str();

                let mut data = ctx.data.lock();
                let subsholder = data.get::<SubsHolder>().unwrap();
                let mut guard = subsholder.lock().unwrap();
                (*guard).filter_names.push(search_str.to_string());

                let filters = (*guard).filter_names.iter().fold(String::new(), | mut res, it | { write!(res, "\"{}\", ", it); res });
                msg.channel_id.say(format!("Ok, currently searching for: {}", filters));
            } else if msg.content == "-help" {
                msg.channel_id.say("Commands:\n-test : Test if the bot is working, responds with \"Hi there!\" if it is.\n-talk_here : Tell the bot to talk on the current channel when it gets new info.\n-search_for <something> : Tell the bot to search for events with <something> in it.\n-clear_searches : Clear all the added searches.");
            } else if msg.content == "-clear_searches" {
                let mut data = ctx.data.lock();
                let subsholder = data.get::<SubsHolder>().unwrap();
                let mut guard = subsholder.lock().unwrap();
                (*guard).filter_names.clear();

                msg.channel_id.say("Filters cleared!");
            }
        }
    }

}

fn poll_site(last_update: Arc<Mutex<DateTime<Utc>>>, channelholder: Arc<Mutex<Option<ChannelId>>>, subsholder: Arc<Mutex<SubsHolder>>) {
    let client = ::hyper::Client::new();

    rt::run(lazy(move || {
        client.get("http://de.twstats.com/de152/index.php?page=ennoblements&live=live".parse().unwrap())
            .and_then(move |res| {
                println!("status: {}", res.status());
                res.into_body().concat2()
            })
            .and_then(move |body| {
                let bodystr = ::std::str::from_utf8(&body)
                    .expect("Invalid encoding sent from server, must be utf-8");

                let twcoll = parse_doc(bodystr);

                // for twevent in &twcoll {
                //     println!("Got event: {:?}", twevent);
                // }

                // {
                //     println!("Checking if channel is available");
                //     let channelholder = channelholder.lock().unwrap();
                //     if let Some(channel) = *channelholder {
                //         channel.say("I got some data!").expect("Failed to communicate with discord, something is wrong!");
                //     }
                //     println!("Channel stuff done");
                // }

                if twcoll.len() > 0 {
                    let message = {
                        let last_update = last_update.lock().unwrap();
                        
                        let subsholder = subsholder.lock().unwrap();

                        let filter_names = &subsholder.filter_names;

                        let last_update_g = *last_update;

                        twcoll
                            .iter()
                            .filter(|it| {
                                // println!("Compare {} > {} = {}", it.time, last_update_g, it.time > last_update_g);
                                it.time > last_update_g
                            })
                            .filter(|it| filter_names.iter().any(|s| it.place.contains(s) || it.old_holder.contains(s) || it.new_holder.contains(s)))
                            .fold(String::new(), |mut res, it| {
                                write!(&mut res, "{} has taken {} from {} at {}!\n", it.new_holder, it.place, it.old_holder, it.time);
                                res
                            })
                    };

                    if message != "" {
                        let channelholder = channelholder.lock().unwrap();
                        let channel = *channelholder;

                        if let Some(channel) = channel {
                            println!("Sending message: {}", &message);
                            channel.say(message);
                        }
                    }
                }

                if let Some(latest) = twcoll.first() {
                    let mut guard = last_update.lock().unwrap();
                    println!("last_update: {:?}, now: {}", *guard, Utc::now());
                    *guard = latest.time;
                }
                
                Ok(())
            })
            .map_err(|err| {
                println!("error: {}", err);
            })}));
}

fn start_discord_bot(discord_client: Client) {
    let mut discord_client = discord_client;
    std::thread::spawn(move || {

        if let Err(why) = discord_client.start() {
            println!("Failed to start bot: {:?}", why);
        }     
    });
}
