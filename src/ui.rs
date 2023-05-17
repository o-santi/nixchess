use crate::db::{Move, Game, InsertionError};
use crate::queries::{game_from_id, movements_from_game, movement_and_games_from_position};
use cursive::event::{Event, Key};
use cursive::theme::{ColorStyle, Color, BaseColor, Style, Effect};
use cursive::view::{Resizable, ScrollStrategy};
use log::{info, trace};
use pgn_reader::{Square, Role, Color as PieceColor};
use shakmaty::{Board, Piece, Chess, Position};
use cursive::views::{Dialog, LinearLayout, EditView, TextView, DummyView, Panel, ScrollView, ProgressBar};
use cursive::traits::Nameable;
use cursive::{Cursive, CursiveExt, View};
use sqlx::Postgres;
use sqlx::pool::PoolConnection;
use sqlx::postgres::PgPoolOptions;
use std::cell::RefCell;
use std::rc::Rc;

pub struct BoardState {
  game: Game,
  moves: Vec<Move>,
  related_games: Vec<Vec<(Move, Game)>>,
  curr_move_idx: usize
}

impl BoardState {
  async fn build(conn: &mut PoolConnection<Postgres>, game: Game) -> Result<Self, InsertionError> {
    info!("fetching moves");
    let moves = movements_from_game(conn, game.id.clone()).await?;
    info!("moves fetched");
    let mut related_games = Vec::with_capacity(moves.len());
    for movement in moves.iter() {
      info!("fetching related games {}", movement.game_round);
      let related_game = if movement.game_round > 5 {
        movement_and_games_from_position(conn, movement.board).await?.into_iter().filter(|(x,_)| x.game_id != game.id).collect()
      } else {
        Vec::new()
      };
      related_games.push(related_game)
    }
    Ok(BoardState {
      game,
      moves,
      related_games,
      curr_move_idx: 0
    })
  }

  fn board(&self) -> Board {
    let mut chess = Chess::default();
    for movement in self.moves.iter().take(self.curr_move_idx) {
      let mov = movement.san_plus.0.san.to_move(&chess).expect("invalid move in database");
      chess.play_unchecked(&mov);
    }
    chess.board().clone()
  }

  // fn curr_square(&self) -> Option<Square> {
  //   let mut chess = Chess::default();
  //   for movement in self.moves.iter().take(self.curr_move_idx) {
  //     let mov = movement.san_plus.0.san.to_move(&chess).expect("invalid move in database");
  //     chess.play_unchecked(&mov);
  //   }
  //   self.moves
  //     .get(self.curr_move_idx - 1)
  //     .map(|mvmt| mvmt.san_plus.0.san.to_move(&chess).expect("invalid move in database").to())
  // }
}

pub fn fetch_game(game_id: i32, db_url: String) -> Result<BoardState, InsertionError> {
  let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().unwrap();
  runtime.block_on(async {
    let pool = PgPoolOptions::new().max_connections(20).connect(&db_url).await?;
    info!("Connected to database");
    let mut conn = pool.acquire().await?;
    info!("Connection acquired");
    let game = game_from_id(&mut conn, game_id).await?;
    info!("Game fetched");
    let board_state = BoardState::build(&mut conn, game).await?;
    info!("board_state built");
    Ok::<BoardState, InsertionError>(board_state)
  })
}

pub fn cli_entrypoint(db_url: String) {
  let mut siv = cursive::default();
  siv.set_window_title("Nixchess");
  siv.set_user_data(db_url);
  siv.add_layer(
    Dialog::around(
      EditView::new()
        .with_name("game_id")
    )
      .title("Enter game id:")
      .button("Ok", |s| {
        let game_id = s.call_on_name("game_id", |v: &mut EditView| v.get_content()).unwrap();
        let db_url = s.take_user_data::<String>().unwrap();
        match game_id.parse::<i32>() {
          Ok(id) => {
            match fetch_game(id, db_url) {
              Ok(board_state) => {
                s.pop_layer();
                show_game(s, Rc::new(RefCell::new(board_state)));
              }
              Err(err) => {
                let debug = Dialog::around(TextView::new(format!("{:?}", err)));
                s.add_layer(debug);
                s.add_global_callback('q', |s| { s.pop_layer(); });
              },
            }
          }
          Err(err) => {
            let debug = Dialog::around(TextView::new(err.to_string()));
            s.add_layer(debug);
            s.add_global_callback('q', |s| { s.pop_layer(); });
          },
        }
      }
        
      ));
  siv.run_crossterm().expect("Could not run on crossterm backend");
}

pub fn show_game(siv: &mut Cursive, board: Rc<RefCell<BoardState>>) {
  let board_view = draw_board_state(&board.borrow_mut());
  siv.add_layer(board_view);
  let board_right = board.clone();
  let board_left = board;
  siv.add_global_callback(Event::Key(Key::Right), move |siv| {
    siv.pop_layer();
    let mut board = board_right.borrow_mut();
    if board.curr_move_idx < board.moves.len() {
      board.curr_move_idx += 1;
    }
    siv.add_layer(draw_board_state(&board));
  });
  
  siv.add_global_callback(Event::Key(Key::Left), move |siv| {
    siv.pop_layer();
    let mut board = board_left.borrow_mut();
    if board.curr_move_idx > 0 {
      board.curr_move_idx -= 1;
    }
    siv.add_layer(draw_board_state(&board));
  });
}

pub fn draw_related_games_column(board_state: &BoardState) -> impl View {
  let related_board = board_state.related_games.get(board_state.curr_move_idx).unwrap();
  let mut lines = LinearLayout::vertical();
  for (played_move, game) in related_board.iter() {
    let mvmt = TextView::new(if played_move.game_round % 2 == 1 {
      format!("{} {}", (played_move.game_round + 1) / 2, played_move.san_plus.0)
    } else {
      format!("{} ... {}", played_move.game_round / 2, played_move.san_plus.0)
    });
    let game = TextView::new(format!("{} ({}) vs {} ({})", game.white, game.white_elo.unwrap(), game.black, game.black_elo.unwrap()));
    let layout = LinearLayout::horizontal().child(mvmt).child(DummyView).child(game);
    lines.add_child(layout);
  }
  let related_games = ScrollView::new(lines).show_scrollbars(true);
  Dialog::around(related_games).title(format!("{} games reach this position", related_board.len())).full_height()
    .max_width(45) // max width of board + movement column combined
    // TODO: figure out a way to calculate this stuff before hand
}
 
pub fn draw_movement_column(board_state: &BoardState) -> impl View {
  let mut white_column = LinearLayout::vertical();
  let mut black_column = LinearLayout::vertical();
  let mut mvmt_count_col = LinearLayout::vertical();
  let seen     = Style { effects: Effect::Dim | Effect::Strikethrough, color: Color::Dark(BaseColor::Black).into() };
  let not_seen = Color::Dark(BaseColor::Black).into();
  for movement in board_state.moves.iter() {
    let style = if (movement.game_round - 1) >= board_state.curr_move_idx as i32 { not_seen } else { seen };
    let mvmt_sans = TextView::new(format!("{}", movement.san_plus.0)).style(style);
    if movement.game_round % 2 == 1 {
      white_column.add_child(mvmt_sans);
      mvmt_count_col.add_child(TextView::new(format!("{}", (movement.game_round + 1)/2)))
    } else {
      black_column.add_child(mvmt_sans);
    }
  }
  let columns = LinearLayout::horizontal()
    .child(mvmt_count_col)
    .child(DummyView)
    .child(white_column)
    .child(DummyView)
    .child(black_column);
  ScrollView::new(columns).show_scrollbars(true).scroll_strategy(ScrollStrategy::KeepRow).max_height(9)
}

pub fn draw_chess_board(board_state: &BoardState) -> impl View {
  let mut board_column = LinearLayout::vertical();
  let light_square = Color::Light(BaseColor::Magenta);
  let dark_square  = Color::Dark(BaseColor::Magenta);
  let chess_board = board_state.board();
  for row in (0..8).rev() {
    let mut row_layout = LinearLayout::horizontal()
      .child(DummyView)
      .child(TextView::new(format!("{}", row + 1)))
      .child(DummyView);
    for col in 0..8 {
      let square = Square::new(row * 8 + col);
      let piece = chess_board.piece_at(square);
      let (piece_color, character) = maybe_piece_to_unicode_color(piece);
      let cell = TextView::new(format!(" {character} ")).style(
        if row % 2 != col % 2 {
          ColorStyle::new(piece_color, light_square)
        } else {
          ColorStyle::new(piece_color, dark_square)
        }
      );
      row_layout.add_child(cell);
    }
    row_layout.add_child(DummyView);
    board_column.add_child(row_layout);
  }
  board_column.add_child(TextView::new("   A  B  C  D  E  F  G  H"));
  board_column
}

pub fn draw_board_state(board_state: &BoardState) -> impl View {
  let game_description = LinearLayout::vertical()
    .child(TextView::new(format!("{} [W] vs {} [B]", board_state.game.white, board_state.game.black)))
    .child(TextView::new(format!("{} {}", board_state.game.event, board_state.game.datetime)));
  let board = draw_chess_board(board_state);
  let movement_column = draw_movement_column(board_state);
  let middle = LinearLayout::horizontal().child(board).child(movement_column);
  let related_games = draw_related_games_column(board_state);
  let main_content = LinearLayout::vertical().child(Panel::new(game_description)).child(middle);
  LinearLayout::vertical().child(Panel::new(main_content)).child(related_games)
}

pub fn maybe_piece_to_unicode_color(piece: Option<Piece>) -> (Color, char) {
  let piece_char  = match piece {
    Some(Piece { role: Role::King, ..   }) => '\u{265A}',
    Some(Piece { role: Role::Queen, ..  }) => '\u{265B}',
    Some(Piece { role: Role::Rook, ..   }) => '\u{265C}',
    Some(Piece { role: Role::Bishop, .. }) => '\u{265D}',
    Some(Piece { role: Role::Knight, .. }) => '\u{265E}',
    Some(Piece { role: Role::Pawn, ..   }) => '\u{265F}',
    None => ' '
  };
  let color = if let Some(Piece {color: PieceColor::White, ..}) = piece {
    Color::Dark(BaseColor::White)
  } else if let Some(Piece {color: PieceColor::Black, ..}) = piece {
    Color::Dark(BaseColor::Black)
  } else {
    Color::TerminalDefault
  };
  (color, piece_char)
}

