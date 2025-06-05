use anyhow::Result;
use chrono::NaiveDateTime;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use sqlx::postgres::PgPool;
use std::env;
use std::fmt;
use std::sync::Arc;

#[derive(Debug, Default, Serialize)]
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

async fn get_res_range(pool: &PgPool, start: i32, end: Option<i32>) -> Result<Vec<Res>> {
    match end {
        Some(end_no) => {
            sqlx::query_as!(
                Res,
                r#"
                    SELECT *
                    FROM res
                    WHERE no >= $1 AND no <= $2
                    ORDER BY no ASC
                "#,
                start,
                end_no
            )
            .fetch_all(pool)
            .await
            .map_err(Into::into)
        }
        None => {
            sqlx::query_as!(
                Res,
                r#"
                    SELECT *
                    FROM res
                    WHERE no = $1
                "#,
                start
            )
            .fetch_all(pool)
            .await
            .map_err(Into::into)
        }
    }
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
        
        let range_regex = Regex::new(r"(\d+)(?:-(\d+))?").unwrap();

        if let Some(captures) = range_regex.captures(&cleaned_content) {
            let start = captures.get(1).unwrap().as_str().parse::<i32>().unwrap();
            let end = captures.get(2).map(|m| m.as_str().parse::<i32>().unwrap());

            match get_res_range(&self.pool, start, end).await {
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
        } else {
            if let Err(e) = msg
                .reply(
                    &ctx.http,
                    "使い方: @bot 123 または @bot 123-128",
                )
                .await
            {
                eprintln!("Error sending message: {:?}", e);
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
