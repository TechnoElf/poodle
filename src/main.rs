use std::thread::sleep;
use std::time::Duration;
use std::sync::Arc;
use std::collections::HashMap;

use serenity::prelude::*;
use serenity::async_trait;
use serenity::model::gateway::Ready;
use serenity::model::channel::*;
use serenity::model::id::*;

use tokio::sync::Mutex;

use rand::{thread_rng, Rng};

use config::*;

mod moodle;
use moodle::*;

#[tokio::main]
async fn main() {
    let mut conf = Config::default();
    if let Err(_) = conf.merge(File::with_name("poodle.toml")) {
        conf.merge(File::with_name("/etc/poodle/poodle.toml")).expect("Config file not found");
    }

    let conf = Conf {
        moodle_auth: MoodleAuthConf::ShibbolethUser(conf.get_str("user").expect("Key \"user\" missing from config"), conf.get_str("pass").expect("Key \"user\" missing from config")),
        discord_token: conf.get_str("token").expect("Key \"token\" missing from config"),
        discord_client_id: conf.get_str("client").expect("Key \"client\" missing from config"),
        responses: conf.get_array("responses").expect("Key \"responses\" missing from config").iter().map(|v| v.clone().into_str().expect("Expected text responses in config")).collect()
    };

    let mut client = Client::builder(conf.discord_token.clone()).event_handler(Handler::new(conf)).await.expect("Failed to construct Discord client");
    client.start().await.expect("Error running Discord client");
}

struct Handler {
    subscribers: Arc<Mutex<HashMap<ChannelId, Vec<MoodleCourseData>>>>,
    conf: Arc<Conf>
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("Connected as {}", ready.user.name);

        let subscribers = self.subscribers.clone();
        let conf = self.conf.clone();

        tokio::spawn(async move { loop {
            let context = MoodleContext::login(&conf.moodle_auth).await.unwrap();

            for (channel, cache) in subscribers.lock().await.iter_mut() {
                for mut course in cache.iter_mut() {
                    if let Some(diff) = context.update(&mut course).await {
                        println!("Update in course {}", course.id());

                        if let Err(e) = channel.send_message(&ctx.http, |m| {
                            m.embed(|e| {
                                e.title(format!("Update in course {}", course.name().clone()));
                                e.url(course.url().clone());
                                e.description(format!("{}\n{}", diff, get_resp(&conf)));
                                e
                            });
                            m
                        }).await {
                            eprintln!("Error sending message: {}", e);
                        }
                    }
                }
            }

            sleep(Duration::from_secs_f32(300.0));
        }});
    }

    async fn message(&self, ctx: Context, msg: Message) {
        let subscribers = self.subscribers.clone();
        let conf = self.conf.clone();

        let words = msg.content.split(" ").collect::<Vec<_>>();
        if (msg.content.starts_with(&format!("<@!{}>", conf.discord_client_id)) ||
            msg.content.starts_with(&format!("<@{}>", conf.discord_client_id))) &&
            words.len() >= 2 {
            let cmd = words[1];

            if cmd == "watch" && words.len() >= 3 {
                for word in words[2..].iter() {
                    if let Ok(id) = word.parse() {
                        match MoodleContext::login(&conf.moodle_auth).await {
                            Ok(context) => {
                                if let Some(course) = context.get(id).await {
                                    let mut subscribers = subscribers.lock().await;
                                    if subscribers.contains_key(&msg.channel_id) {
                                        if let None = subscribers.get(&msg.channel_id).unwrap().iter().position(|e| e.id() == id) {
                                            subscribers.get_mut(&msg.channel_id).unwrap().push(course);
                                        }
                                    } else {
                                        subscribers.insert(msg.channel_id, vec![course]);
                                    }

                                    if let Err(e) = msg.channel_id.say(&ctx.http, get_resp(&conf)).await {
                                        eprintln!("Error sending message: {}", e);
                                    }
                                    println!("Channel {} is watching course {}", msg.channel_id, id);
                                } else {
                                    eprintln!("Failed to fetch course data for {}", id)
                                }
                            },
                            Err(_) => eprintln!("Failed to access Moodle")
                        }
                    }
                }
            }

            if cmd == "unwatch" && words.len() >= 3 {
                for word in words[2..].iter() {
                    if let Ok(id) = word.parse::<u32>() {
                        if let Some(cache) = self.subscribers.lock().await.get_mut(&msg.channel_id) {
                            if let Some(course_index) = cache.iter().position(|e| e.id() == id) {
                                cache.remove(course_index);

                                if let Err(e) = msg.channel_id.say(&ctx.http, get_resp(&conf)).await {
                                    eprintln!("Error sending message: {}", e);
                                }
                                println!("Channel {} is no longer watching course {}", msg.channel_id, id);
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Handler {
    fn new(conf: Conf) -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(HashMap::new())),
            conf: Arc::new(conf)
        }
    } 
}

fn get_resp(conf: &Conf) -> &str {
    conf.responses.get(thread_rng().gen_range(0, conf.responses.len())).expect("Expected at least one response text")
}

struct Conf {
    moodle_auth: MoodleAuthConf,
    discord_token: String,
    discord_client_id: String,
    responses: Vec<String>
}
