use crate::db::{InsertionError, Game, Move, GameId, SAN};
use sqlx::pool::PoolConnection;
use shakmaty::san::SanPlus;
use sqlx::Postgres;


// pub async fn analysis(db: Pool<sqlx::Postgres>, game: Game) -> Result<(), InsertionError> {
//   let mut conn = db.acquire().await.expect("Could not acquire pool connection");
//   let moves = movements_from_game(&mut conn, game.id.clone()).await?;

//   println!("Tournament: {}", game.event);
//   println!("{} vs {}", game.white, game.black);
//   println!("");

//   let mut board = Chess::default();
  
//   for movement in moves {
//     if movement.game_round % 2 == 0 {
//       println!("{} . {}", movement.game_round / 2, movement.san_plus.0);
//     } else {
//       println!("... {}", movement.san_plus.0);
//     }
//     println!("");

//     board.play_unchecked(&movement.san_plus.0.san.to_move(&board).expect("valid move"));
//     print_board(board.board(), movement.clone());
    
//     let moves_from_position = movements_from_position(&mut conn, movement.board).await?;
//     if moves_from_position.len() > 1 {
//       println!("{} other games reach this position.", moves_from_position.len() - 1);
//     }
//     for possible_move in moves_from_position.into_iter().filter(|x| x.game_id != game.id).take(5) {
//       let played_in_game = game_from_move(&mut conn, possible_move.clone()).await?;
//       println!("---- {} played in {} vs {} ({})", possible_move.san_plus.0, played_in_game.white, played_in_game.black, played_in_game.datetime);
//     }
//     println!();
//   }
//   Ok(())
// }

pub async fn games_from_player(db: &mut PoolConnection<Postgres>, player: &str) -> Result<Vec<Game>, InsertionError> {
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


pub async fn movements_from_game(db: &mut PoolConnection<Postgres>, game_id: GameId) -> Result<Vec<Move>, InsertionError> {
  let row = sqlx::query!(
    r#"SELECT game_round, game_id, san_plus, fen FROM (Move INNER JOIN Board ON board = id) WHERE game_id = ($1) ORDER BY game_round"#,
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

pub async fn movements_from_position(db: &mut PoolConnection<Postgres>, board: String) -> Result<Vec<Move>, InsertionError> {
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

pub async fn game_from_move(db: &mut PoolConnection<Postgres>, movement: Move) -> Result<Game, InsertionError> {
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

pub async fn game_from_id(db: &mut PoolConnection<Postgres>, game_id: i32) -> Result<Game, InsertionError> {
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
