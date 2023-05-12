use crate::db::{Move, GameId, Game, InsertionError};
use crate::queries::{game_from_id, movements_from_game};
use cursive::event::{Event, Key};
use cursive::theme::{ColorStyle, Color, ColorType, BaseColor};
use cursive::view::Resizable;
use pgn_reader::{Square, Role, Color as PieceColor, San};
use shakmaty::{Board, Piece, Chess, Position};
use cursive::views::{Dialog, LinearLayout, EditView, TextView, DummyView, ScrollView};
use cursive::traits::Nameable;
use cursive::{Cursive, View};
use sqlx::Postgres;
use sqlx::pool::PoolConnection;
use sqlx::postgres::PgPoolOptions;
use std::borrow::Borrow;
use std::cell::RefCell;
use std::rc::Rc;


pub struct BoardState {
  game: Game,
  moves: Vec<Move>,
  curr_move_idx: usize
}

impl BoardState {
  async fn build(conn: &mut PoolConnection<Postgres>, game: Game) -> Result<Self, InsertionError> {
    let moves = movements_from_game(conn, game.id.clone()).await.expect("movements");
    Ok(BoardState {
      game,
      moves,
      curr_move_idx: 0
    })
  }

  fn board(&self) -> Board {
    let mut chess = Chess::default();
    for movement in self.moves.iter().take(self.curr_move_idx) {
      let mov = movement.san_plus.0.san.to_move(&chess).expect("valid move");
      chess.play_unchecked(&mov);
    }
    chess.board().clone()
  }
}

pub fn fetch_game(game_id: i32) -> Result<BoardState, InsertionError> {
  let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
  dotenv::dotenv().ok();
  let db_url = std::env::var("DATABASE_URL").expect("Unable to read DATABASE_URL env var");
  let pool = runtime.block_on(PgPoolOptions::new().max_connections(20).connect(&db_url))?;
  let mut conn = runtime.block_on(pool.acquire())?;
  let game = runtime.block_on(game_from_id(&mut conn, game_id))?;
  let board_state = runtime.block_on(BoardState::build(&mut conn, game))?;
  Ok(board_state)
}

pub fn cli_entrypoint() {
  let mut siv = cursive::default();
  siv.add_layer(
    Dialog::around(
      EditView::new()
        .with_name("game_id")
    )
      .title("Enter game id:")
      .button("Ok", |s| {
        let game_id = s.call_on_name("game_id", |v: &mut EditView| v.get_content()).unwrap();
        if let Ok(id) = game_id.parse::<i32>() {
          if let Ok(board_state) = fetch_game(id) {
            show_game(s, Rc::new(RefCell::new(board_state)));
          }
        }
      }));
  siv.run();
}

pub fn show_game(siv: &mut Cursive, board: Rc<RefCell<BoardState>>) {
  siv.pop_layer();
  let board_view = draw_board(&board.borrow_mut());
  siv.add_layer(board_view);
  let board_right = board.clone();
  let board_left = board;
  siv.add_global_callback(Event::Key(Key::Right), move |siv| {
    let mut board = board_right.borrow_mut();
    if board.curr_move_idx < board.moves.len() {
      board.curr_move_idx += 1;
    }
    siv.pop_layer();
    siv.add_layer(draw_board(&board));
  });
  
  siv.add_global_callback(Event::Key(Key::Left), move |siv| {
    let mut board = board_left.borrow_mut();
    if board.curr_move_idx > 0 {
      board.curr_move_idx -= 1;
    }
    siv.pop_layer();
    siv.add_layer(draw_board(&board));
  });
}

pub fn draw_board(board_state: &BoardState) -> impl View {
  let mut board_column = LinearLayout::vertical()
    .child(TextView::new(format!("{} [W] vs {} [B] at {} ({})",
                                 board_state.game.white, board_state.game.black, board_state.game.event, board_state.game.datetime)).max_width(40))
    .child(DummyView);
  let white_style = ColorStyle::new(Color::TerminalDefault, ColorType::Color(Color::Light(BaseColor::Black)));
  let black_style = ColorStyle::new(Color::TerminalDefault, Color::Dark(BaseColor::Black));
  for row in (0..8).rev() {
    let mut row_layout = LinearLayout::horizontal()
      .child(TextView::new(format!("{} ", row + 1)));
    for col in 0..8 {
      let square = Square::new(row * 8 + col);
      let piece = board_state.board().piece_at(square);
      let character = maybe_piece_to_unicode(piece);
      let cell = TextView::new(format!(" {character} ")).style(
        if row % 2 != col % 2 {
          white_style
        } else {
          black_style
        }
      );
      row_layout.add_child(cell);
    }
    row_layout.add_child(DummyView);
    board_column.add_child(row_layout);
  }
  board_column.add_child(TextView::new("   A  B  C  D  E  F  G  H"));

  let mut white_column = LinearLayout::vertical();
  let mut black_column = LinearLayout::vertical();
  let seen     = ColorStyle::new(Color::Light(BaseColor::White), Color::Rgb(135, 135, 180));
  let not_seen = ColorStyle::new(Color::TerminalDefault, Color::Rgb(45, 45, 60));
  for movement in board_state.moves.iter() {
    let style = if movement.game_round >= board_state.curr_move_idx as i32 { not_seen } else { seen };
    if movement.game_round % 2 == 0 {
      let row = TextView::new(format!("{}. {} ", movement.game_round / 2, movement.san_plus.0)).style(style);
      white_column.add_child(row);
    } else {
      let row = TextView::new(format!(" {}", movement.san_plus.0)).style(style);
      black_column.add_child(row);
    }
  }
  let movement_colum = LinearLayout::horizontal().child(white_column).child(black_column);
  LinearLayout::horizontal().child(board_column).child(movement_colum)
}

pub fn maybe_piece_to_unicode(piece: Option<Piece>) -> char {
  match piece {
    Some(Piece { color: PieceColor::White, role: Role::King   }) => '\u{2654}',
    Some(Piece { color: PieceColor::White, role: Role::Queen  }) => '\u{2655}',       
    Some(Piece { color: PieceColor::White, role: Role::Rook   }) => '\u{2656}',
    Some(Piece { color: PieceColor::White, role: Role::Bishop }) => '\u{2657}',
    Some(Piece { color: PieceColor::White, role: Role::Knight }) => '\u{2658}',
    Some(Piece { color: PieceColor::White, role: Role::Pawn   }) => '\u{2659}',
    Some(Piece { color: PieceColor::Black, role: Role::King   }) => '\u{265A}',
    Some(Piece { color: PieceColor::Black, role: Role::Queen  }) => '\u{265B}',
    Some(Piece { color: PieceColor::Black, role: Role::Rook   }) => '\u{265C}',
    Some(Piece { color: PieceColor::Black, role: Role::Bishop }) => '\u{265D}',
    Some(Piece { color: PieceColor::Black, role: Role::Knight }) => '\u{265E}',
    Some(Piece { color: PieceColor::Black, role: Role::Pawn   }) => '\u{265F}',
    None => ' '
  }
}

pub fn print_board(board: &Board,last_move: Move) {
  for row in (0..8).rev() {
    print!("{} |", row + 1);
    for col in 0..8 {
      let square = Square::new(row * 8 + col);
      let piece = board.piece_at(square);
      let character = match piece {
        Some(Piece { color: PieceColor::White, role: Role::King   }) => "\u{2654}",
        Some(Piece { color: PieceColor::White, role: Role::Queen  }) => "\u{2655}",       
        Some(Piece { color: PieceColor::White, role: Role::Rook   }) => "\u{2656}",
        Some(Piece { color: PieceColor::White, role: Role::Bishop }) => "\u{2657}",
        Some(Piece { color: PieceColor::White, role: Role::Knight }) => "\u{2658}",
        Some(Piece { color: PieceColor::White, role: Role::Pawn   }) => "\u{2659}",
        Some(Piece { color: PieceColor::Black, role: Role::King   }) => "\u{265A}",
        Some(Piece { color: PieceColor::Black, role: Role::Queen  }) => "\u{265B}",
        Some(Piece { color: PieceColor::Black, role: Role::Rook   }) => "\u{265C}",
        Some(Piece { color: PieceColor::Black, role: Role::Bishop }) => "\u{265D}",
        Some(Piece { color: PieceColor::Black, role: Role::Knight }) => "\u{265E}",
        Some(Piece { color: PieceColor::Black, role: Role::Pawn   }) => "\u{265F}",
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

