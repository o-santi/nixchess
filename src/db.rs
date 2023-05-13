use shakmaty::fen::Fen;
use shakmaty::{Chess, Position};
use pgn_reader::{RawHeader, SanPlus, Visitor, BufferedReader};
use sqlx::types::chrono::{NaiveDate, NaiveTime, NaiveDateTime};
use sqlx::{Error as DbErr, PgConnection, Acquire, Pool, Postgres};
use sqlx::pool::PoolConnection;


#[derive(Debug, Clone)]
pub struct Game {
  pub id: GameId,
  pub event: String,
  pub datetime: NaiveDateTime,
  pub white: String,
  pub black: String,
}


#[derive(sqlx::Type, Debug, Clone, sqlx::FromRow, PartialEq)]
pub struct GameId {
  pub id: i32,
}

#[derive(Debug)]
pub enum InsertionError {
  DbError(DbErr),
  ParsingError,
  IncompleteDataError(String),
  IlegalMove(SanPlus),
}


#[derive(Debug)]
pub struct PGNParser {
  event: Option<String>,
  date: Option<NaiveDate>,
  time: Option<NaiveTime>,
  white: Option<String>,
  black: Option<String>,
  white_elo: Option<usize>,
  black_elo: Option<usize>,
  moves: Vec<SAN>,
}

#[derive(Debug)]
pub struct ParsedChessGame {
  event: String,
  pub date: NaiveDate,
  pub time: NaiveTime,
  pub white: String,
  pub black: String,
  moves: Vec<SAN>,
}

#[derive(Debug, Clone)]
pub struct SAN(pub SanPlus);

impl sqlx::Encode<'_, sqlx::Postgres> for SAN {
  fn encode_by_ref(
    &self,
    buf: &mut <sqlx::Postgres as sqlx::database::HasArguments<'_>>::ArgumentBuffer,
  ) -> sqlx::encode::IsNull {
    format!("{}", self.0).encode_by_ref(buf)
  }
}
impl sqlx::Type<sqlx::Postgres> for SAN {
  fn type_info() -> <sqlx::Postgres as sqlx::Database>::TypeInfo {
    <&str as sqlx::Type<sqlx::Postgres>>::type_info()
  }
}

#[derive(Debug, Clone)]
pub struct Move {
  pub board: String, // board id
  pub san_plus: SAN,
  pub game_id: GameId,
  pub game_round: i32
}

impl PGNParser {
  pub fn new() -> Self {
    PGNParser {
      event: None,
      date: None,
      time: None,
      white: None,
      black: None,
      white_elo: None,
      black_elo: None,
      moves: Vec::new(),
    }
  }
}

impl Visitor for PGNParser {
  type Result = ParsedChessGame;

  fn header(&mut self, key: &[u8], value: RawHeader<'_>) {
    let val: String = value.decode_utf8_lossy().into();
    //println!("{}:{val}", std::str::from_utf8(key).unwrap());
    match key {
      b"Event" => self.event = Some(val),
      b"UTCDate" => {
        let date = NaiveDate::parse_from_str(&val, "%Y.%m.%d").expect("could not parse date string");
        self.date = Some(date)
      }
      b"UTCTime" => {
        let time = NaiveTime::parse_from_str(&val, "%H:%M:%S").expect("could not parse time string");
        self.time = Some(time)
      }
      b"White" => self.white = Some(val),
      b"Black" => self.black = Some(val),
      // b"WhiteElo" => {
      //   let elo = val.parse::<usize>().unwrap();
      //   self.white_elo = Some(elo);
      // }
      // b"BlackElo" => {
      //   let elo = val.parse::<usize>().unwrap();
      //   self.black_elo = Some(elo);
      // }
      _ => {}
    }
  }

  fn san(&mut self, san: SanPlus) {
    self.moves.push(SAN(san));
  }

  fn end_game(&mut self) -> Self::Result {
    let game = std::mem::replace(self, Self::new());
    ParsedChessGame {
      event: game.event.expect("Event missing"),
      date: game.date.expect("Date missing"),
      time: game.time.expect("Time missing"),
      white: game.white.expect("White player missing"),
      black: game.black.expect("Black player missing"),
      moves: game.moves,
    }
  }
}

async fn insert_player(db: &mut PgConnection, name: String) -> Result<(), InsertionError> {
  sqlx::query!(
    r#"INSERT INTO Player (player_name) VALUES ($1) ON CONFLICT DO NOTHING"#,
    name
  ).execute(db)
    .await?;
  Ok(())
}

impl ParsedChessGame {
  
  pub async fn insert(self, mut pool: PoolConnection<sqlx::Postgres>) -> Result<(), InsertionError> {
    let mut tx = pool.begin().await?;
    insert_player(&mut tx, self.white.clone()).await?;
    insert_player(&mut tx, self.black.clone()).await?;
    let datetime: NaiveDateTime = NaiveDateTime::new(self.date, self.time);
    let game_id = sqlx::query_as!(
      GameId,
      r#"INSERT INTO Game (white, black, event, datetime) VALUES ($1, $2, $3, $4) RETURNING id;"#,
      self.white,
      self.black,
      self.event,
      datetime
    )
      .fetch_one(&mut tx)
      .await?
      .clone();
    let mut board = Chess::default();
    for (index, movement) in self.moves.into_iter().enumerate() {
      let move_to_play = movement
        .0
        .san
        .to_move(&board)
        .map_err(|_| InsertionError::IlegalMove(movement.0.clone()))?
        .clone();
      let before_board = board.clone();
      board = board
        .play(&move_to_play)
        .map_err(|_| InsertionError::IlegalMove(movement.0.clone()))?;
      let fen = Fen::from_position(before_board, shakmaty::EnPassantMode::Always);
      let fen_str = format!("{fen}");
      let maybe_fen_id = sqlx::query!(
        r#"SELECT id from Board WHERE fen = ($1);"#, fen_str
      ).fetch_optional(&mut tx)
        .await?;
      let fen_id = match maybe_fen_id {
        Some(row) => row.id,
        None => {
          let fetch = sqlx::query!(
            r#"INSERT INTO Board (fen) VALUES ($1) RETURNING ID"#,
            fen_str
          ).fetch_one(&mut tx)
            .await?;
          fetch.id
        }
      };
      sqlx::query!(
        r#"INSERT INTO Move (game_round, game_id, san_plus, board) VALUES ($1, $2, $3, $4);"#,
        index as i32,
        game_id.id,
        format!("{}", movement.0),
        fen_id
      ).execute(&mut tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
  }
}

impl From<DbErr> for InsertionError {
  fn from(value: DbErr) -> Self {
    InsertionError::DbError(value)
  }
}

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

pub async fn insert_games_from_file(pool: &Pool<Postgres>, file: &str) -> Result<(), InsertionError> {
  println!("Parsing the games from file.");
  let games = parse_lichess_pgn(file)?;
  println!("Parsed!");
  let mut tasks = Vec::new();
  for game in games {
    let task = tokio::spawn(game.insert(pool.acquire().await.expect("Could not acquire handle")));
    tasks.push(task)
  }
  for task in tasks {
    let _ = task.await.map_err(|err| eprintln!("{}", err));
  }
  Ok(())
}
