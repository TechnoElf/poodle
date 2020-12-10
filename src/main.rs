use std::thread::sleep;
use std::time::Duration;
use std::sync::Arc;
use std::collections::HashMap;
use std::fs::read_to_string;

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
    match read_to_string("./poodle.toml") {
        Ok(c) => { conf.merge(File::from_str(&c, FileFormat::Toml)).unwrap(); },
        Err(_) => { conf.merge(File::from_str(&read_to_string("/etc/poodle/poodle.toml").expect("Config file not found"), FileFormat::Toml)).unwrap(); }
    }

    let auth = MoodleAuthConf::ShibbolethUser(conf.get_str("user").expect("Key \"user\" missing from config"), conf.get_str("pass").expect("Key \"user\" missing from config"));
    let conf = Conf {
        discord_token: conf.get_str("token").expect("Key \"token\" missing from config"),
        discord_client_id: conf.get_str("client").expect("Key \"client\" missing from config"),
        responses: conf.get_array("responses").expect("Key \"responses\" missing from config").iter().map(|v| v.clone().into_str().expect("Expected text responses in config")).collect()
    };

    let mut client = Client::builder(conf.discord_token.clone()).event_handler(Handler::new(conf, auth)).await.expect("Failed to construct Discord client");
    client.start().await.expect("Error running Discord client");
}

struct Handler {
    context: Arc<Mutex<MoodleContext>>,
    subscribers: Arc<Mutex<HashMap<ChannelId, Vec<MoodleCourseData>>>>,
    conf: Arc<Conf>,
    groups: Arc<Mutex<Vec<String>>>
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("Connected as {}", ready.user.name);

        let context = self.context.clone();
        let subscribers = self.subscribers.clone();
        let conf = self.conf.clone();

        tokio::spawn(async move { loop {
            for (channel, cache) in subscribers.lock().await.iter_mut() {
                for mut course in cache.iter_mut() {
                    if let Ok(Some(diff)) = context.lock().await.update(&mut course).await {
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
        let context = self.context.clone();
        let subscribers = self.subscribers.clone();
        let conf = self.conf.clone();
        let groups = self.groups.clone();

        let words = msg.content.split(" ").collect::<Vec<_>>();
        if (msg.content.starts_with(&format!("<@!{}>", conf.discord_client_id)) ||
            msg.content.starts_with(&format!("<@{}>", conf.discord_client_id))) &&
            words.len() >= 2 {
            let cmd = words[1];

            if cmd == "watch" && words.len() >= 3 {
                for word in words[2..].iter() {
                    if let Ok(id) = word.parse() {
                        if let Ok(course) = context.lock().await.get(id).await {
                            let mut subscribers = subscribers.lock().await;
                            if subscribers.contains_key(&msg.channel_id) {
                                if let None = subscribers.get(&msg.channel_id).unwrap().iter().position(|e| e.id() == id) {
                                    subscribers.get_mut(&msg.channel_id).unwrap().push(course);
                                }
                            } else {
                                subscribers.insert(msg.channel_id, vec![course]);
                            }

                            if let Err(e) = msg.channel_id.say(&ctx.http, format!("{} (watching course {})", get_resp(&conf), id)).await {
                                eprintln!("Error sending message: {}", e);
                            }
                            println!("Channel {} is watching course {}", msg.channel_id, id);
                        } else {
                            eprintln!("Failed to fetch course data for {}", id)
                        }
                    }
                }
            } else if cmd == "unwatch" && words.len() >= 3 {
                for word in words[2..].iter() {
                    if let Ok(id) = word.parse::<u32>() {
                        if let Some(cache) = self.subscribers.lock().await.get_mut(&msg.channel_id) {
                            if let Some(course_index) = cache.iter().position(|e| e.id() == id) {
                                cache.remove(course_index);

                                if let Err(e) = msg.channel_id.say(&ctx.http, format!("{} (no longer watching course {})", get_resp(&conf), id)).await {
                                    eprintln!("Error sending message: {}", e);
                                }
                                println!("Channel {} is no longer watching course {}", msg.channel_id, id);
                            }
                        }
                    }
                }
            } else if cmd == "timer" && words.len() == 3 {
                if let Ok(time) = words[2].parse() {
                    if let Err(e) = msg.channel_id.say(&ctx.http, format!("{} (timer set for {} seconds)", get_resp(&conf), time)).await {
                        eprintln!("Error sending message: {}", e);
                    }

                    tokio::spawn(async move {
                        sleep(Duration::from_secs(time));

                        if let Err(e) = msg.channel_id.say(&ctx.http, format!("{} (timer done)", get_resp(&conf))).await {
                            eprintln!("Error sending message: {}", e);
                        }
                    });
                }
            } else if cmd == "makegroups" && words.len() == 2 {
                let mut groups = groups.lock().await.clone();
                let group_size = (groups.len() as f32 * 2.0).log2().floor() as usize;
                let group_count = groups.len() as usize / group_size;

                // Random permutation by Fisher-Yates Shuffle
                let mut perm: Vec<Vec<String>> = Vec::with_capacity(group_count);
                for _ in 0..group_count {
                    perm.push(Vec::with_capacity(group_size));
                }

                for i in 0..groups.len() {
                    let j = thread_rng().gen_range(0, groups.len());
                    perm[i % group_count].push(groups.remove(j));
                }

                if let Err(e) = msg.channel_id.say(&ctx.http, get_resp(&conf)).await {
                    eprintln!("Error sending message: {}", e);
                }

                let mut text = String::new();
                for (i, group) in perm.iter().enumerate() {
                    let mut substring = String::new();
                    for e in group {
                        substring.push_str(&format!("{} ", e));
                    }
                    text.push_str(&format!("Group {}: {}\n", i + 1, substring));
                }

                if let Err(e) = msg.channel_id.say(&ctx.http, text).await {
                    eprintln!("Error sending message: {}", e);
                }
            } else if cmd == "makegroups" && words.len() >= 3 {
                let mut groups = groups.lock().await;

                groups.clear();
                for word in words[2..].iter() {
                    groups.push(word.to_string());
                }

                if let Err(e) = msg.channel_id.say(&ctx.http, format!("{} (created group with {} members)", get_resp(&conf), groups.len())).await {
                    eprintln!("Error sending message: {}", e);
                }
            }
        }
    }
}

impl Handler {
    fn new(conf: Conf, auth: MoodleAuthConf) -> Self {
        Self {
            context: Arc::new(Mutex::new(MoodleContext::new(auth))),
            subscribers: Arc::new(Mutex::new(HashMap::new())),
            conf: Arc::new(conf),
            groups: Arc::new(Mutex::new(Vec::new()))
        }
    } 
}

fn get_resp(conf: &Conf) -> &str {
    conf.responses.get(thread_rng().gen_range(0, conf.responses.len())).expect("Expected at least one response text")
}

struct Conf {
    discord_token: String,
    discord_client_id: String,
    responses: Vec<String>
}
