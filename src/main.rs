use sqlx::postgres::PgPoolOptions;
//use nixchess::queries::{analysis, games_from_player};
use nixchess::ui::cli_entrypoint;

fn main() {
  // dotenv::dotenv().ok();
  // let db_url = std::env::var("DATABASE_URL").expect("Unable to read DATABASE_URL env var");
  // println!("Connecting to database.");
  // let pool = PgPoolOptions::new().max_connections(20).connect(&db_url).await.expect("Could not connect to the database");
  // println!("Connected!");
  // //insert_games_from_file(&pool, "./lichess_db_standard_rated_2013-01.pgn").await.unwrap();
  // let mut conn = pool.acquire().await.unwrap();
  // let games = games_from_player(&mut conn, "Kozakmamay007").await.unwrap();
  // analysis(pool, games.get(0).unwrap().clone()).await.unwrap();
  cli_entrypoint();
}
