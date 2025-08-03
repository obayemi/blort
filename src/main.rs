use axum::{extract::{Path, State}, routing::get, Router};
use sqlx::PgPool;
use std::net::SocketAddr;

async fn hello_world() -> &'static str {
    "Hello, world!"
}

async fn hello_name(
    Path(name): Path<String>,
    State(db): State<PgPool>,
) -> Result<String, String> {
    // First, try to get existing record
    let existing = sqlx::query!(
        "SELECT count, last_seen FROM items WHERE name = $1",
        name
    )
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    
    let db = PgPool::connect(&database_url).await?;
    
    sqlx::migrate!().run(&db).await?;

    let router = Router::new()
        .route("/", get(hello_world))
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
