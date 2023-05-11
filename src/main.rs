use nixchess::{ParsedChessGame, InsertionError, PGNParser, games_from_player, movements_from_game, Game, print_game, analysis};
use pgn_reader::BufferedReader;
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};

fn parse_lichess_pgn(filepath: &str) -> Result<Vec<ParsedChessGame>, InsertionError> {
  let game = std::fs::read_to_string(filepath).expect("Could not find file");
  let mut reader = BufferedReader::new_cursor(&game);
  let mut visitor = PGNParser::new();
  let mut ret = Vec::new();
  while let Some(game) = reader
    .read_game(&mut visitor)
    .expect("Could not read pgn file")
  {
    ret.push(game);
  }
  Ok(ret)
}

async fn insert_games_from_file(pool: &Pool<Postgres>, file: &str) -> Result<(), InsertionError> {
  println!("Parsing the games from file.");
  let games = parse_lichess_pgn(file)?;
  println!("Parsed!");
  let mut tasks = Vec::new();
  for game in games {
    let task = tokio::spawn(game.insert(pool.acquire().await.expect("Could not acquire handle")));
    tasks.push(task)
  }
  for task in tasks {
    task.await.expect("Could not join threads")?;
  }
  Ok(())
}

#[tokio::main]
async fn main() {
  dotenv::dotenv().ok();
  let db_url = std::env::var("DATABASE_URL").expect("Unable to read DATABASE_URL env var");
  println!("Connecting to database.");
  let pool = PgPoolOptions::new().max_connections(20).connect(&db_url).await.expect("Could not connect to the database");
  println!("Connected!");
  //insert_games_from_file(&pool, "./lichess_db_standard_rated_2013-01.pgn").await.unwrap();
  let mut conn = pool.acquire().await.unwrap();
  let games = games_from_player(&mut conn, "Kozakmamay007").await.unwrap();
  analysis(pool, games.get(0).unwrap().clone()).await.unwrap();
}
