use axum::{
    extract::{Path, State},
    routing::get,
    Router,
};
use clap::{Parser, Subcommand, ValueEnum};
use sqlx::{types::chrono, PgPool};
use std::net::SocketAddr;

#[derive(Parser)]
#[command(name = "blort")]
#[command(about = "A name tracking web application")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(ValueEnum, Clone)]
enum OrderBy {
    LastSeen,
    Visits,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the web server
    Run,
    /// Clear all data from the database
    Clear,
    /// Show recent names with their statistics
    Show {
        /// Number of names to show (default: 10)
        #[arg(short, long, default_value_t = 10)]
        limit: u32,
        /// Order results by last_seen or visits
        #[arg(short, long, value_enum, default_value_t = OrderBy::LastSeen)]
        order: OrderBy,
    },
}

async fn hello_world() -> &'static str {
    "Hello, world!"
}

async fn health_check() -> &'static str {
    "OK"
}

async fn hello_name(Path(name): Path<String>, State(db): State<PgPool>) -> Result<String, String> {
    // First, try to get existing record
    let existing = sqlx::query!("SELECT count, last_seen FROM items WHERE name = $1", name)
        .fetch_optional(&db)
        .await
        .map_err(|e| format!("Database error: {}", e))?;

    let (previous_count, last_seen) = match existing {
        Some(record) => (record.count, Some(record.last_seen)),
        None => (0, None),
    };

    // Update or insert the record
    sqlx::query!(
        "INSERT INTO items (name, count, last_seen) VALUES ($1, 1, NOW())
         ON CONFLICT (name) DO UPDATE SET 
         count = items.count + 1, 
         last_seen = NOW()",
        name
    )
    .execute(&db)
    .await
    .map_err(|e| format!("Database error: {}", e))?;

    let response = if let Some(last_seen) = last_seen {
        format!(
            "Hello {}! You've been called {} times previously. Last seen: {}",
            name, previous_count, last_seen
        )
    } else {
        format!("Hello {}! This is your first visit!", name)
    };

    Ok(response)
}

async fn run_server(db: PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let router = Router::new()
        .route("/", get(hello_world))
        .route("/ok", get(health_check))
        .route("/hello/{name}", get(hello_name))
        .with_state(db);

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3001".to_string())
        .parse::<u16>()
        .expect("PORT must be a valid number");

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("Server running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}

async fn clear_database(db: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query!("TRUNCATE TABLE items").execute(db).await?;
    println!("Database cleared successfully");
    Ok(())
}

async fn show_names(
    db: &PgPool,
    limit: u32,
    order: OrderBy,
) -> Result<(), Box<dyn std::error::Error>> {
    let sort_label = match order {
        OrderBy::LastSeen => "last seen",
        OrderBy::Visits => "visits",
    };

    let rows = match order {
        OrderBy::LastSeen => {
            sqlx::query!("SELECT name, count, last_seen FROM items ORDER BY last_seen DESC LIMIT $1", limit as i32)
                .fetch_all(db)
                .await?
        }
        OrderBy::Visits => {
            sqlx::query!("SELECT name, count, last_seen FROM items ORDER BY count DESC LIMIT $1", limit as i32)
                .fetch_all(db)
                .await?
        }
    };

    if rows.is_empty() {
        println!("No names found in database");
        return Ok(());
    }

    println!("Top {} names (sorted by {}):", limit, sort_label);
    println!("{:<20} {:<8} {}", "Name", "Visits", "Last Seen");
    println!("{}", "-".repeat(50));

    for row in rows {
        println!(
            "{:<20} {:<8} {}",
            row.name,
            row.count,
            row.last_seen.format("%Y-%m-%d %H:%M:%S")
        );
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let db = PgPool::connect(&database_url).await?;

    sqlx::migrate!().run(&db).await?;

    match cli.command {
        Commands::Run => run_server(db).await?,
        Commands::Clear => clear_database(&db).await?,
        Commands::Show { limit, order } => show_names(&db, limit, order).await?,
    }

    Ok(())
}
