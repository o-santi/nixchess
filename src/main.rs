use nixchess::{ChessGame, InsertionError, PGNParser};
use pgn_reader::BufferedReader;
use sqlx::{PgConnection, Connection};

fn parse_lichess_pgn(filepath: &str) -> Result<Vec<ChessGame>, InsertionError> {
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

#[tokio::main]
async fn main() {
  dotenv::dotenv().ok();
  let db_url = std::env::var("DATABASE_URL").expect("Unable to read DATABASE_URL env var");
  let mut db = PgConnection::connect(&db_url).await.expect("Could not connect to the database");
  let games = parse_lichess_pgn("./lichess_db_standard_rated_2013-01.pgn").unwrap();
  for game in games {
    let (white, black, date, time) = (game.white.clone(), game.black.clone(), game.date, game.time);
    game.insert(&mut db).await.expect("Could not insert game");
    println!("{white} vs {black} ({date} {time})")
  }
}
