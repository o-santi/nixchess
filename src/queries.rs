use crate::db::{InsertionError, Game, Move, GameId, SAN};
use sqlx::PgConnection;
use shakmaty::{san::SanPlus, zobrist::Zobrist64};
use futures_util::TryStreamExt;

pub async fn games_from_player(db: &mut PgConnection, player: &str) -> Result<Vec<Game>, InsertionError> {
  let games = sqlx::query!(
    r#"SELECT id, event, datetime, black, white, white_elo, black_elo FROM Game WHERE black = ($1) OR white = ($1)"#,
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
      white_elo: row.white_elo,
      black_elo: row.black_elo,
    })
    .collect();
  Ok(games)
}


pub async fn movements_from_game(db: &mut PgConnection, game_id: GameId) -> Result<Vec<Move>, InsertionError> {
  let row = sqlx::query!(
    r#"SELECT game_round, game_id, san_plus, board_hash FROM Move WHERE game_id = ($1) ORDER BY game_round"#,
    game_id.id
  ).fetch_all(db)
    .await?;
  let moves = row.into_iter().map(|row| Move {
    board: Zobrist64(row.board_hash as u64),
    san_plus: SAN(SanPlus::from_ascii(row.san_plus.as_bytes()).unwrap()),
    game_id: GameId { id: row.game_id },
    game_round: row.game_round,
  }).collect();
  Ok(moves)
}

pub async fn movement_and_games_from_position(db: &mut PgConnection, board_hash: Zobrist64) -> Result<Vec<(Move, Game)>, InsertionError> {
  let row = sqlx::query!(
    r#"SELECT game_round, game_id, san_plus, board_hash, white, black, event, datetime, white_elo, black_elo FROM (Move INNER JOIN Game ON game_id = id) WHERE board_hash = ($1)"#,
    board_hash.0 as i64
  ).fetch_all(db)
    .await?;
  let moves = row.into_iter().map(|row| {
    let game_id = GameId { id: row.game_id };
    (Move {
      board: Zobrist64(row.board_hash as u64),
      san_plus: SAN(SanPlus::from_ascii(row.san_plus.as_bytes()).unwrap()),
      game_id: game_id.clone(),
      game_round: row.game_round,
    }, Game {
      id: game_id,
      event: row.event,
      datetime: row.datetime,
      white: row.white,
      black: row.black,
      white_elo: row.white_elo,
      black_elo: row.black_elo,
    })
  }).collect();
  Ok(moves)
}

pub async fn game_from_move(db: &mut PgConnection, movement: Move) -> Result<Game, InsertionError> {
  let row = sqlx::query!(
    r#"SELECT id, white, black, event, datetime, white_elo, black_elo from Game INNER JOIN move ON id = game_id WHERE id = ($1)"#,
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
    white_elo: row.white_elo,
    black_elo: row.black_elo,
  };
  Ok(game)
}

pub async fn game_from_id(db: &mut PgConnection, game_id: i32) -> Result<Game, InsertionError> {
  let row = sqlx::query!(
    r#"SELECT id, white, black, event, datetime, white_elo, black_elo from Game WHERE id = ($1)"#,
    game_id
  )
    .fetch_one(db)
    .await?;
  let game = Game {
    id: GameId { id: row.id },
    event: row.event,
    datetime: row.datetime,
    white: row.white,
    black: row.black,
    white_elo: row.white_elo,
    black_elo: row.black_elo 
  };
  Ok(game)
}

pub async fn related_games_from_game(conn: &mut PgConnection, game_id: i32) -> Result<Vec<Vec<(Move, Game)>>, InsertionError> {
  let mut query = sqlx::query!(
    r#"WITH game_moves as (
        SELECT game_round, board_hash FROM Move WHERE game_id = ($1) AND game_round > 6
    )
       SELECT Related.game_round, Related.board_hash, Related.game_id, Related.san_plus, black, white, datetime, id, event, white_elo, black_elo
       FROM (Move as Related INNER JOIN game_moves ON (Related.board_hash = game_moves.board_hash) INNER JOIN Game ON Related.game_id = id)
       WHERE Related.game_id != ($1)
    "#, game_id
  ).fetch(conn);
  let mut ret = Vec::new();
  while let Ok(Some(row)) = query.try_next().await {
    let game = Game {
      id: GameId { id: game_id },
      event: row.event,
      datetime: row.datetime,
      white: row.white,
      black: row.black,
      white_elo: row.white_elo,
      black_elo: row.black_elo,
    };
    let mvmt = Move {
      board: Zobrist64(row.game_round as u64),
      san_plus: SAN(SanPlus::from_ascii(row.san_plus.as_bytes()).unwrap()),
      game_id: GameId { id: row.game_id },
      game_round: row.game_round,
    };
    let move_idx = (row.game_round - 1) as usize;
    ret.resize(move_idx + 1, Vec::new());
    let vec = ret.get_mut(move_idx).expect("Vector should always have an element");
    vec.push((mvmt, game));
  }
  
  Ok(ret)
}


