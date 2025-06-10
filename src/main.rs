use anyhow::Result;
use chrono::NaiveDateTime;
use regex::Regex;
use serde::Serialize;
use sqlx::FromRow;
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use sqlx::postgres::PgPool;
use std::collections::HashSet;
use std::env;
use std::fmt;
use std::sync::Arc;

#[derive(Debug, Default, Serialize, FromRow)]
pub struct Res {
    pub no: i32,
    pub name_and_trip: String,
    pub datetime: NaiveDateTime,
    pub datetime_text: String,
    pub id: String,
    pub main_text: String,
    pub main_text_html: String,
    pub oekaki_id: Option<i32>,
}

impl fmt::Display for Res {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "### __{} {} {} ID: {}__\n{}\n",
            self.no, self.name_and_trip, self.datetime_text, self.id, self.main_text
        )
    }
}

async fn get_res_by_numbers(pool: &PgPool, numbers: Vec<i32>) -> Result<Vec<Res>> {
    if numbers.is_empty() {
        return Ok(Vec::new());
    }
    
    let query = "SELECT * FROM res WHERE no = ANY($1) ORDER BY no ASC";
    
    sqlx::query_as::<_, Res>(query)
        .bind(&numbers)
        .fetch_all(pool)
        .await
        .map_err(Into::into)
}

#[derive(Debug)]
enum RangeSpec {
    Include(i32, Option<i32>),
    Exclude(i32, Option<i32>),
}

fn parse_range_specifications(input: &str) -> Vec<RangeSpec> {
    let mut specs = Vec::new();
    let parts: Vec<&str> = input.split(',').collect();
    
    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        
        let (is_exclude, range_str) = if trimmed.starts_with('^') {
            (true, &trimmed[1..])
        } else {
            (false, trimmed)
        };
        
        if let Some(dash_pos) = range_str.find('-') {
            let start_str = &range_str[..dash_pos];
            let end_str = &range_str[dash_pos + 1..];
            
            if let (Ok(start), Ok(end)) = (start_str.parse::<i32>(), end_str.parse::<i32>()) {
                if is_exclude {
                    specs.push(RangeSpec::Exclude(start, Some(end)));
                } else {
                    specs.push(RangeSpec::Include(start, Some(end)));
                }
            }
        } else if let Ok(num) = range_str.parse::<i32>() {
            if is_exclude {
                specs.push(RangeSpec::Exclude(num, None));
            } else {
                specs.push(RangeSpec::Include(num, None));
            }
        }
    }
    
    specs
}

fn calculate_post_numbers(specs: Vec<RangeSpec>) -> Vec<i32> {
    let mut included = HashSet::new();
    let mut excluded = HashSet::new();
    
    for spec in specs {
        match spec {
            RangeSpec::Include(start, end) => {
                if let Some(end_num) = end {
                    for i in start..=end_num {
                        included.insert(i);
                    }
                } else {
                    included.insert(start);
                }
            }
            RangeSpec::Exclude(start, end) => {
                if let Some(end_num) = end {
                    for i in start..=end_num {
                        excluded.insert(i);
                    }
                } else {
                    excluded.insert(start);
                }
            }
        }
    }
    
    let mut result: Vec<i32> = included.difference(&excluded).cloned().collect();
    result.sort();
    result
}

#[derive(Clone)]
struct Bot {
    pool: Arc<PgPool>,
}

#[async_trait]
impl EventHandler for Bot {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot || !msg.mentions_me(&ctx.http).await.unwrap_or(false) {
            return;
        }

        // Remove mentions from content
        let content = msg.content.clone();
        let mention_regex = Regex::new(r"<@!?\d+>").unwrap();
        let cleaned_content = mention_regex.replace_all(&content, "").trim().to_string();
        
        // Parse range specifications
        let specs = parse_range_specifications(&cleaned_content);
        
        if specs.is_empty() {
            if let Err(e) = msg
                .reply(
                    &ctx.http,
                    "使い方: @bot 123 または @bot 123-128 または @bot 123,124-128,^126-127",
                )
                .await
            {
                eprintln!("Error sending message: {:?}", e);
            }
            return;
        }
        
        let post_numbers = calculate_post_numbers(specs);
        
        if post_numbers.is_empty() {
            if let Err(e) = msg
                .reply(&ctx.http, "指定された範囲には表示するレスがありません。")
                .await
            {
                eprintln!("Error sending message: {:?}", e);
            }
            return;
        }

        match get_res_by_numbers(&self.pool, post_numbers).await {
                Ok(posts) => {
                    if posts.is_empty() {
                        if let Err(e) = msg
                            .reply(&ctx.http, "指定された範囲のレスが見つかりませんでした。")
                            .await
                        {
                            eprintln!("Error sending message: {:?}", e);
                        }
                    } else {
                        let mut response = String::new();
                        for post in posts.iter() {
                            response.push_str(&format!("{}\n", post));
                            if response.len() > 1800 {
                                response.push_str("...(表示制限により省略)");
                                break;
                            }
                        }

                        if let Err(e) = msg.reply(&ctx.http, response).await {
                            eprintln!("Error sending message: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Database error: {:?}", e);
                    if let Err(e) = msg
                        .reply(&ctx.http, "データベースエラーが発生しました。")
                        .await
                    {
                        eprintln!("Error sending message: {:?}", e);
                    }
                }
            }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().expect("Failed to load .env file");

    let discord_token = env::var("DISCORD_TOKEN").expect("Expected DISCORD_TOKEN in environment");
    let database_url = env::var("DATABASE_URL").expect("Expected DATABASE_URL in environment");

    let pool = PgPool::connect(&database_url).await?;

    let bot = Bot {
        pool: Arc::new(pool),
    };

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&discord_token, intents)
        .event_handler(bot)
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        eprintln!("Client error: {:?}", why);
    }

    Ok(())
}
