# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Discord bot that fetches posts from a BBS (bulletin board system) stored in PostgreSQL and replies to specified post ranges in Discord using the Serenity framework.

## Build and Development Commands

```bash
# Build the project
cargo build

# Run the project
cargo run

# Build for release
cargo build --release

# Run tests
cargo test

# Check code without building
cargo check

# Format code
cargo fmt

# Run linter
cargo clippy
```

## Commit Guidelines

Before committing any changes, always run the following commands to ensure code quality:

```bash
# Format code
cargo fmt

# Run clippy to check for common mistakes and improve code
cargo clippy
```

If these commands report any issues, fix them before committing. This helps maintain consistent code style and prevents CI failures.

## Architecture

### Core Components

1. **Discord Bot Framework**: Uses Serenity 0.12.4 for Discord integration
   - Handles Discord events and commands
   - Manages message replies and interactions

2. **Database Integration**: Intended to use sqlx for PostgreSQL connectivity
   - Fetches BBS posts from PostgreSQL database
   - Handles post range queries

3. **Async Runtime**: Uses Tokio with multi-threaded runtime
   - Required for both Serenity and sqlx async operations

### Key Dependencies to Add

When implementing the PostgreSQL integration, add to Cargo.toml:
```toml
sqlx = { version = "0.7", features = ["runtime-tokio", "postgres", "macros"] }
```

### Implementation Notes

- The bot should parse Discord commands to extract post range specifications
- Database queries should fetch posts within the specified range
- Responses should be formatted appropriately for Discord's message limits
- Consider implementing pagination for large post ranges