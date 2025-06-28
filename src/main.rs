use std::env;
use std::sync::Arc;

use anyhow::Result;
use bbs_fetch_post_discord_bot::{
    calculate_post_numbers, get_max_post_number, get_res_by_numbers, parse_range_specifications,
    RangeSpec,
};
use log::{debug, error, info};
use regex::Regex;
use serenity::async_trait;
use serenity::builder::{CreateEmbed, CreateMessage};
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use sqlx::postgres::PgPool;

#[derive(Clone)]
struct Bot {
    pool: Arc<PgPool>,
    image_url_prefix: String,
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
        debug!("Input: '{cleaned_content}', Parsed specs: {specs:?}");

        if specs.is_empty() {
            if let Err(e) = msg
                .reply(
                    &ctx.http,
                    "使い方: @fetch-post 123 または @fetch-post 123-128 または @fetch-post 123- または @fetch-post 123,124-128 または @fetch-post ^322,?324-326,?^325",
                )
                .await
            {
                error!("Error sending message: {e:?}");
            }
            return;
        }

        // Check if any spec requires max post number
        let needs_max = specs.iter().any(|spec| {
            debug!("Checking spec {spec:?} for needs_max");
            matches!(
                spec,
                RangeSpec::IncludeFrom(_)
                    | RangeSpec::ExcludeFrom(_)
                    | RangeSpec::RelativeInclude(_, _, _)
                    | RangeSpec::RelativeExclude(_, _, _)
                    | RangeSpec::RelativeIncludeFrom(_, _)
                    | RangeSpec::RelativeExcludeFrom(_, _)
            )
        });
        debug!("needs_max = {needs_max}");

        let max_post_number = if needs_max {
            match get_max_post_number(&self.pool).await {
                Ok(max) => {
                    debug!("Got max post number: {max}");
                    max
                }
                Err(e) => {
                    error!("Error getting max post number: {e:?}");
                    if let Err(e) = msg
                        .reply(&ctx.http, "データベースエラーが発生しました。")
                        .await
                    {
                        error!("Error sending message: {e:?}");
                    }
                    return;
                }
            }
        } else {
            0 // Won't be used if not needed
        };

        let post_numbers = calculate_post_numbers(specs, max_post_number);
        debug!("Calculated post numbers: {post_numbers:?}");

        if post_numbers.is_empty() {
            if let Err(e) = msg
                .reply(&ctx.http, "指定された範囲には表示するレスがありません。")
                .await
            {
                error!("Error sending message: {e:?}");
            }
            return;
        }

        match get_res_by_numbers(&self.pool, post_numbers.clone()).await {
            Ok(posts) => {
                debug!("Got {} posts for numbers {:?}", posts.len(), post_numbers);
                if posts.is_empty() {
                    if let Err(e) = msg
                        .reply(&ctx.http, "指定された範囲のレスが見つかりませんでした。")
                        .await
                    {
                        error!("Error sending message: {e:?}");
                    }
                } else {
                    // Send posts with images if they have oekaki_id
                    let mut current_message = String::new();

                    for post in posts.iter() {
                        let post_text = format!("{post}");

                        // Check if adding this post would exceed Discord's limit
                        if !current_message.is_empty()
                            && current_message.len() + post_text.len() > 1800
                        {
                            // Send the current batch
                            if let Err(e) = msg.reply(&ctx.http, &current_message).await {
                                error!("Error sending message: {e:?}");
                            }
                            current_message.clear();
                        }

                        current_message.push_str(&post_text);

                        // Send image if oekaki_id exists
                        if let Some(oekaki_id) = post.oekaki_id {
                            // Send current text if any
                            if !current_message.is_empty() {
                                if let Err(e) = msg.reply(&ctx.http, &current_message).await {
                                    error!("Error sending message: {e:?}");
                                }
                                current_message.clear();
                            }

                            // Send image as embed
                            let image_url = format!("{}{}.png", self.image_url_prefix, oekaki_id);
                            let builder = CreateMessage::new()
                                .reference_message(&msg)
                                .embed(CreateEmbed::new().image(image_url));

                            if let Err(e) = msg.channel_id.send_message(&ctx.http, builder).await {
                                eprintln!("Error sending image: {e:?}");
                            }
                        }
                    }

                    // Send any remaining text
                    if !current_message.is_empty() {
                        if let Err(e) = msg.reply(&ctx.http, current_message).await {
                            error!("Error sending message: {e:?}");
                        }
                    }
                }
            }
            Err(e) => {
                error!("Database error: {e:?}");
                if let Err(e) = msg
                    .reply(&ctx.http, "データベースエラーが発生しました。")
                    .await
                {
                    error!("Error sending message: {e:?}");
                }
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().expect("Failed to load .env file");

    // Initialize logger with RUST_LOG environment variable
    env_logger::init();

    let discord_token = env::var("DISCORD_TOKEN").expect("Expected DISCORD_TOKEN in environment");
    let database_url = env::var("DATABASE_URL").expect("Expected DATABASE_URL in environment");
    let image_url_prefix =
        env::var("IMAGE_URL_PREFIX").expect("Expected IMAGE_URL_PREFIX in environment");

    let pool = PgPool::connect(&database_url).await?;

    let bot = Bot {
        pool: Arc::new(pool),
        image_url_prefix,
    };

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&discord_token, intents)
        .event_handler(bot)
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        error!("Client error: {why:?}");
    }

    Ok(())
}
