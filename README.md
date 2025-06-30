# bbs-fetch-post-discord-bot

[![CI](https://github.com/toof-jp/bbs-fetch-post-discord-bot/workflows/CI/badge.svg)](https://github.com/toof-jp/bbs-fetch-post-discord-bot/actions)

A Discord bot that fetches posts from a BBS (bulletin board system) stored in PostgreSQL and replies with the specified post numbers' content on Discord.

## Overview

When you mention the bot on Discord and specify post numbers, it replies with the corresponding post content. You can specify single posts, ranges, exclusions, and other flexible post number specifications.

## Post Number Specification

### Single Post

To specify a single post number:

```
@bot 123
```

- Displays only post number 123

### Range Specification

To specify a range of consecutive post numbers:

#### Closed Range

```
@bot 123-128
```

- Displays all posts from 123 to 128 (123, 124, 125, 126, 127, 128)

#### Open Range

```
@bot 123-
```

- Displays all posts from 123 to the latest post
- Automatically fetches up to the maximum post number in the database

### Exclusion Specification

To exclude specific post numbers, use `^` (caret):

#### Single Exclusion

```
@bot 123-128,^126
```

- Displays posts from 123 to 128, but excludes 126
- Result: 123, 124, 125, 127, 128

#### Range Exclusion

```
@bot 100-200,^150-160
```

- Displays posts from 100 to 200, but excludes 150 to 160
- Result: Posts 100-149, 161-200

#### Open Range Exclusion

```
@bot 1-,^500-
```

- Displays posts from 1 to latest, but excludes 500 and later
- Result: Posts 1-499

### Relative Reference (? Prefix)

Using `?` (question mark) allows you to specify relative post numbers based on the upper digits of the current maximum post number. The number of digits for relative reference is automatically adjusted based on the number of digits after `?`:

#### Single Relative Reference

```
@bot ?324
```

- If the current maximum post number is 123340, displays post number 123324
- `?324` has 3 digits, so it replaces the last 3 digits of the maximum post number with 324

```
@bot ?24
```

- If the current maximum post number is 123340, displays post number 123324
- `?24` has 2 digits, so it replaces the last 2 digits of the maximum post number with 24

```
@bot ?1234
```

- If the current maximum post number is 1234567, displays post number 1231234
- `?1234` has 4 digits, so it replaces the last 4 digits of the maximum post number with 1234

#### Relative Reference Range

```
@bot ?324-326
```

- If the current maximum post number is 123340, displays posts from 123324 to 123326

#### Relative Reference Exclusion

```
@bot ?320-330,?^325
```

- If the current maximum post number is 123340, displays posts from 123320 to 123330, but excludes 123325

#### Relative Reference Open Range

```
@bot ?300-
```

- If the current maximum post number is 123340, displays posts from 123300 to the latest (123340)

#### Relative Reference Wraparound Behavior

When a calculated post number from relative reference exceeds the maximum post number, it automatically wraps around to the previous base value:

```
@bot ?456
```

- If maximum post number is 2345:
  - Normal calculation: 2000 + 456 = 2456 (exceeds maximum)
  - After wraparound: 1000 + 456 = 1456
- If maximum post number is 123456:
  - Calculation result: 123000 + 456 = 123456 (within maximum, no wraparound)

### Combined Specification

Combine multiple conditions by separating them with commas (`,`):

```
@bot 10,20-25,30,^23
```

- Displays posts 10, 20-25, and 30, but excludes 23
- Result: 10, 20, 21, 22, 24, 25, 30

```
@bot 1-50,100-150,^25-30,^125-130
```

- Displays posts 1-50 and 100-150, but excludes 25-30 and 125-130
- Result: 1-24, 31-50, 100-124, 131-150

```
@bot ?324,100-110,?^326-328
```

- If the current maximum post number is 123340:
  - Displays post 123324 and posts 100-110, but excludes 123326-123328
  - Result: 100-110, 123324

## Technical Specifications

### Processing Flow

1. **Mention Detection**: Detects messages where the bot is mentioned
2. **Parse Processing**: Parses post number specifications from the message
3. **Number Calculation**:
   - Creates a set of post numbers to include
   - Creates a set of post numbers to exclude
   - Generates the final post number list using set operations (difference)
4. **Database Retrieval**: Fetches post data from PostgreSQL based on the calculated post number list
5. **Reply**: Replies with the fetched post content on Discord

### Limitations

- Due to Discord's character limit, messages exceeding 1800 characters are split and sent separately
- If specified post numbers don't exist, only existing posts are displayed
- When using open ranges (e.g., `123-`) or relative references (e.g., `?324`), additional database queries are required
- Posts with images are sent as separate messages, which may affect display order
- Relative references replace the lower digits of the maximum post number according to the specified number of digits. If the maximum post number has fewer digits than specified, the relative reference is treated as a regular post number

### Database Schema

Post data is stored with the following structure:

```sql
CREATE TABLE res (
    no INTEGER PRIMARY KEY,           -- Post number
    name_and_trip TEXT NOT NULL,      -- Poster name and trip
    datetime TIMESTAMP NOT NULL,      -- Post datetime
    datetime_text TEXT NOT NULL,      -- Post datetime (text format)
    id TEXT NOT NULL,                 -- Poster ID
    main_text TEXT NOT NULL,          -- Post content (plain text)
    main_text_html TEXT NOT NULL,     -- Post content (HTML)
    oekaki_id INTEGER                 -- Drawing ID (optional)
);
```

### Output Format

Each post is displayed in the following format:

```
### __[Post Number] [Poster Name] [Post DateTime] ID: [Poster ID]__
[Post Content]
```

## Image Post Support

When a post contains a drawing ID (`oekaki_id`), the corresponding image is automatically displayed as a Discord embed. The image URL is generated by combining the environment variable `IMAGE_URL_PREFIX` with the `oekaki_id`.

Example: If `IMAGE_URL_PREFIX` is `https://example.com/images/` and `oekaki_id` is `123`, the image URL becomes `https://example.com/images/123.png`.

## Environment Configuration

The following environment variables must be set in the `.env` file:

```env
DISCORD_TOKEN=your_discord_bot_token
DATABASE_URL=postgresql://username:password@host:port/database
IMAGE_URL_PREFIX=https://example.com/images/

# Log level configuration (optional)
# Available values: error, warn, info, debug, trace
# Default: info
RUST_LOG=info
```

### Log Level Configuration

You can control the log verbosity with the `RUST_LOG` environment variable:

- `RUST_LOG=error`: Show only errors
- `RUST_LOG=warn`: Show warnings and errors
- `RUST_LOG=info`: Show info, warnings, and errors (default)
- `RUST_LOG=debug`: Show all logs including debug information
- `RUST_LOG=trace`: Show the most detailed logs

You can also set log levels for specific modules individually:

```env
# Show debug logs for the bot, but only info level for serenity
RUST_LOG=bbs_fetch_post_discord_bot=debug,serenity=info
```

## Dependencies

- **Serenity 0.12.4**: Discord API communication
- **SQLx 0.7**: Asynchronous PostgreSQL database communication
- **Tokio**: Asynchronous runtime
- **Others**: regex (regular expressions), chrono (datetime handling), anyhow (error handling), etc.

## Build and Run

```bash
# Run in development environment
cargo run

# Release build
cargo build --release

# Run tests
cargo test

# Format code
cargo fmt

# Run linter
cargo clippy
```