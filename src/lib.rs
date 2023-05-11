use pgn_reader::{RawHeader, SanPlus, Visitor, Square, Color, Role, San};
use shakmaty::fen::Fen;
use shakmaty::{Chess, Position, Board, Piece};
use sqlx::pool::PoolConnection;
use sqlx::postgres::PgRow;
use sqlx::types::chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use sqlx::{Error as DbErr, PgConnection, Connection, PgPool, Pool, Transaction};

#[derive(Debug)]
pub enum InsertionError {
  DbError(DbErr),
  ParsingError,
  IncompleteDataError(String),
  IlegalMove(SanPlus),
}

#[derive(sqlx::Type, Debug, Clone, sqlx::FromRow, PartialEq)]
pub struct GameId {
  id: i32,
}

#[derive(Debug, Clone)]
pub struct Game {
  pub id: GameId,
  event: String,
  datetime: NaiveDateTime,
  white: String,
  black: String,
}

pub fn print_game(game: Game, moves: Vec<Move>) {
  println!("{} vs {} @ ({})", game.white, game.black, game.datetime);
  println!("-----------------------------");
  for (round, movement) in moves.into_iter().enumerate() {
    if round % 2 == 0 {
      print!("{} . {}", round/2 + 1, movement.san_plus.0.san);
    } else {
      println!(" {}", movement.san_plus.0.san)
    }
  }
  println!("-----------------------------");
}

#[derive(Debug, Clone)]
struct SAN(SanPlus);

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
  board: String, // board id
  san_plus: SAN,
  game_id: GameId,
  game_round: i32
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

pub async fn games_from_player(db: &mut PoolConnection<sqlx::Postgres>, player: &str) -> Result<Vec<Game>, InsertionError> {
  let games = sqlx::query!(
    r#"SELECT id, event, datetime, black, white FROM Game WHERE black = ($1) OR white = ($1)"#,
    player
  ).fetch_all(db)
    .await?
    .into_iter()
    .map(|row| Game {
      id: GameId { id: row.id },
      event: row.event,
      datetime: row.datetime,
      white: row.white,
      black: row.black,
    })
    .collect();
  Ok(games)
}


pub async fn movements_from_game(db: &mut PoolConnection<sqlx::Postgres>, game_id: GameId) -> Result<Vec<Move>, InsertionError> {
  let row = sqlx::query!(
    r#"SELECT game_round, game_id, san_plus, fen FROM (Move INNER JOIN Board ON board = id) WHERE game_id = ($1)"#,
    game_id.id
  ).fetch_all(db)
    .await?;
  let moves = row.into_iter().map(|row| Move {
    board: row.fen,
    san_plus: SAN(SanPlus::from_ascii(row.san_plus.as_bytes()).unwrap()),
    game_id: GameId { id: row.game_id },
    game_round: row.game_round,
  }).collect();
  Ok(moves)
}

pub async fn movements_from_position(db: &mut PoolConnection<sqlx::Postgres>, board: String) -> Result<Vec<Move>, InsertionError> {
  let row = sqlx::query!(
    r#"SELECT game_round, game_id, san_plus, fen FROM (Move INNER JOIN Board ON board = id) WHERE fen = ($1)"#,
    board
  ).fetch_all(db)
    .await?;
  let moves = row.into_iter().map(|row| Move {
    board: row.fen,
    san_plus: SAN(SanPlus::from_ascii(row.san_plus.as_bytes()).unwrap()),
    game_id: GameId { id: row.game_id },
    game_round: row.game_round,
  }).collect();
  Ok(moves)
}

pub async fn game_from_move(db: &mut PoolConnection<sqlx::Postgres>, movement: Move) -> Result<Game, InsertionError> {
  let row = sqlx::query!(
    r#"SELECT id, white, black, event, datetime from Game INNER JOIN move ON id = game_id WHERE id = ($1)"#,
    movement.game_id.id
  )
    .fetch_one(db)
    .await?;
  let game = Game {
    id: GameId { id: row.id },
    event: row.event,
    datetime: row.datetime,
    white: row.white,
    black: row.black,    
  };
  Ok(game)
}

pub async fn analysis(db: Pool<sqlx::Postgres>, game: Game) -> Result<(), InsertionError> {
  let mut conn = db.acquire().await.expect("Could not acquire pool connection");
  let moves = movements_from_game(&mut conn, game.id.clone()).await?;

  println!("Tournament: {}", game.event);
  println!("{} vs {}", game.white, game.black);
  println!("");

  let mut board = Chess::default();
  
  for movement in moves {
    if movement.game_round % 2 == 0 {
      println!("{} . {}", movement.game_round / 2, movement.san_plus.0);
    } else {
      println!("... {}", movement.san_plus.0);
    }
    println!("");

    board.play_unchecked(&movement.san_plus.0.san.to_move(&board).expect("valid move"));
    print_board(board.board(), movement.clone());
    
    let moves_from_position = movements_from_position(&mut conn, movement.board).await?;
    if moves_from_position.len() > 1 {
      println!("{} other games reach this position.", moves_from_position.len() - 1);
    }
    for possible_move in moves_from_position.into_iter().filter(|x| x.game_id != game.id).take(5) {
      let played_in_game = game_from_move(&mut conn, possible_move.clone()).await?;
      println!("---- {} played in {} vs {} ({})", possible_move.san_plus.0, played_in_game.white, played_in_game.black, played_in_game.datetime);
    }
    println!();
  }
  Ok(())
}

fn print_board(board: &Board,last_move: Move) {
  for row in (0..8).rev() {
    print!("{} |", row + 1);
    for col in 0..8 {
      let square = Square::new(row * 8 + col);
      let piece = board.piece_at(square);
      let character = match piece {
        Some(Piece { color: Color::White, role: Role::King   }) => "\u{2654}",
        Some(Piece { color: Color::White, role: Role::Queen  }) => "\u{2655}",       
        Some(Piece { color: Color::White, role: Role::Rook   }) => "\u{2656}",
        Some(Piece { color: Color::White, role: Role::Bishop }) => "\u{2657}",
        Some(Piece { color: Color::White, role: Role::Knight }) => "\u{2658}",
        Some(Piece { color: Color::White, role: Role::Pawn   }) => "\u{2659}",
        Some(Piece { color: Color::Black, role: Role::King   }) => "\u{265A}",
        Some(Piece { color: Color::Black, role: Role::Queen  }) => "\u{265B}",
        Some(Piece { color: Color::Black, role: Role::Rook   }) => "\u{265C}",
        Some(Piece { color: Color::Black, role: Role::Bishop }) => "\u{265D}",
        Some(Piece { color: Color::Black, role: Role::Knight }) => "\u{265E}",
        Some(Piece { color: Color::Black, role: Role::Pawn   }) => "\u{265F}",
        None => " "
      };
      if let San::Normal { to , .. } =  last_move.san_plus.0.san {
        if to == square {
          print!("\x1b[48;5;3m {character} \x1b[0m");
          continue;
        }  
      }
      if row % 2 != col %2 {
        print!("\x1b[48;5;236m {character} \x1b[0m");
      }
      else {
        print!("\x1b[48;5;240m {character} \x1b[0m");
      }
    }
    println!();
  }
  println!("  | A  B  C  D  E  F  G  H")
}
