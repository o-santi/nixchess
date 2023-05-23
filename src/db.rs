use futures_util::stream::FuturesUnordered;
use futures_util::StreamExt;
use shakmaty::{Chess, Position, zobrist::{ZobristHash, Zobrist64}};
use pgn_reader::{RawHeader, SanPlus, Visitor, BufferedReader};
use sqlx::{types::chrono::{NaiveDate, NaiveTime, NaiveDateTime}, PgPool};
use sqlx::Error as DbErr;
use kdam::{BarExt, Column};


#[derive(Debug, Clone)]
pub struct Game {
  pub id: GameId,
  pub event: String,
  pub datetime: NaiveDateTime,
  pub white: String,
  pub black: String,
  pub white_elo: Option<i32>,
  pub black_elo: Option<i32>
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
  IoError(std::io::Error)
}


#[derive(Debug)]
pub struct PGNParser {
  event: Option<String>,
  date: Option<NaiveDate>,
  time: Option<NaiveTime>,
  white: Option<String>,
  black: Option<String>,
  white_elo: Option<i32>,
  black_elo: Option<i32>,
  moves: Vec<SAN>,
}

#[derive(Debug)]
pub struct ParsedChessGame {
  event: String,
  pub date: NaiveDate,
  pub time: NaiveTime,
  pub white: String,
  pub black: String,
  pub white_elo: Option<i32>,
  pub black_elo: Option<i32>,
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
  pub board: Zobrist64, // board id
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
        let date = NaiveDate::parse_from_str(&val, "%Y.%m.%d").expect("Invalid data: could not parse date string");
        self.date = Some(date)
      }
      b"UTCTime" => {
        let time = NaiveTime::parse_from_str(&val, "%H:%M:%S").expect("Invalid data: could not parse time string");
        self.time = Some(time)
      }
      b"White" => self.white = Some(val),
      b"Black" => self.black = Some(val),
      b"WhiteElo" => {
        let elo = val.parse::<i32>().ok();
        self.white_elo = elo;
      }
      b"BlackElo" => {
        let elo = val.parse::<i32>().ok();
        self.black_elo = elo;
      }
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
      white_elo: game.white_elo,
      black_elo: game.black_elo,
      moves: game.moves,
    }
  }
}

impl ParsedChessGame {
  
  pub async fn insert(self, conn: PgPool) -> Result<(), InsertionError> {
    let datetime: NaiveDateTime = NaiveDateTime::new(self.date, self.time);
    let mut board = Chess::default();
    let mut board_hashes = Vec::with_capacity(self.moves.len());
    let mut mvmts = Vec::with_capacity(self.moves.len());
    let mut game_rounds = Vec::with_capacity(self.moves.len());
    for (index, movement) in self.moves.into_iter().enumerate() {
      let move_to_play = movement.0.san.to_move(&board)
        .map_err(|_| InsertionError::IlegalMove(movement.0.clone()))?
        .clone();
      let board_hash = board.zobrist_hash::<Zobrist64>(shakmaty::EnPassantMode::Legal).0;
      board.play_unchecked(&move_to_play);
      board_hashes.push(board_hash as i64);
      mvmts.push(format!("{}", movement.0));
      game_rounds.push((index + 1) as i32)
    }
    sqlx::query!(
      r#"WITH white_player AS (
           INSERT INTO Player VALUES ($1), ($2)
           ON CONFLICT DO NOTHING RETURNING player_name
         ), gid AS (
           INSERT INTO Game (white, black, event, datetime, white_elo, black_elo)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id
         )
         INSERT INTO Move (game_round, san_plus, board_hash, game_id)
         SELECT * FROM UNNEST($7::int[], $8::text[], $9::bigint[])
         CROSS JOIN gid"#,
      self.white,
      self.black,
      self.event,
      datetime,
      self.white_elo,
      self.black_elo,
      &game_rounds,
      &mvmts,
      &board_hashes)
      .execute(&conn)
      .await?;
    Ok(())
  }
}

impl From<DbErr> for InsertionError {
  fn from(value: DbErr) -> Self {
    InsertionError::DbError(value)
  }
}
impl From<std::io::Error> for InsertionError {
  fn from(value: std::io::Error) -> Self {
    InsertionError::IoError(value)
  }
}

pub async fn insert_games_from_file(conn: PgPool, file: &str) -> Result<(), InsertionError> {
  let game_file = std::fs::read_to_string(file)?;
  let reader = BufferedReader::new_cursor(&game_file);
  let mut visitor = PGNParser::new();
  let games = reader
    .into_iter(&mut visitor)
    .filter_map(|x| x.ok())
    .map(|game| {
      tokio::spawn(game.insert(conn.clone()))
    });
  let mut futures = FuturesUnordered::from_iter(games);
  let mut pb = kdam::tqdm!(total=0);
  let mut games = 0;
  while let Some(ret) = futures.next().await {
    pb.update(1);
    if let Err(err) = ret {
      println!("{err:?}");
    } else {
      games += 1;
    }
  }
  println!("");
  println!("{} games inserted in {:.2} seconds.", games, pb.elapsed_time);
  Ok(())
}
