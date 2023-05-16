use crate::db::{Move, Game, InsertionError};
use crate::queries::{game_from_id, movements_from_game, movements_from_position};
use cursive::event::{Event, Key};
use cursive::theme::{ColorStyle, Color, ColorType, BaseColor};
use cursive::view::Resizable;
use pgn_reader::{Square, Role, Color as PieceColor};
use shakmaty::{Board, Piece, Chess, Position};
use cursive::views::{Dialog, LinearLayout, EditView, TextView, DummyView, Panel, ScrollView, ProgressBar};
use cursive::traits::Nameable;
use cursive::{Cursive, View};
use sqlx::Postgres;
use sqlx::pool::PoolConnection;
use sqlx::postgres::PgPoolOptions;
use std::cell::RefCell;
use std::rc::Rc;


pub struct BoardState {
  game: Game,
  moves: Vec<Move>,
  suggestions: Vec<Vec<(Move, Game)>>,
  curr_move_idx: usize
}

impl BoardState {
  async fn build(conn: &mut PoolConnection<Postgres>, game: Game) -> Result<Self, InsertionError> {
    let board_moves = movements_from_game(conn, game.id.clone()).await?;
    let mut moves = Vec::with_capacity(board_moves.len());
    let mut suggestions = Vec::with_capacity(board_moves.len());
    for movement in board_moves.into_iter() {
      let played_here: Vec<Move> = movements_from_position(conn, movement.board.clone()).await?.into_iter().filter(|x| x.game_id != game.id).collect();
      let mut related_moves = Vec::new();
      for played_move in played_here {
        let game = game_from_id(conn, played_move.game_id.id).await?;
        related_moves.push((played_move, game));
      }
      moves.push(movement);
      suggestions.push(related_moves);
    }
    Ok(BoardState {
      game,
      moves,
      suggestions,
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
}

pub fn fetch_game(game_id: i32, db_url: String) -> Result<BoardState, InsertionError> {
  let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
  runtime.block_on(async {
    let pool = PgPoolOptions::new().max_connections(20).connect(&db_url).await?;
    let mut conn = pool.acquire().await?;
    let game = game_from_id(&mut conn, game_id).await?;
    let board_state = BoardState::build(&mut conn, game).await?;
    Ok::<BoardState, InsertionError>(board_state)
  })
}

pub fn cli_entrypoint(db_url: String) {
  let mut siv = cursive::default();
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
  siv.run();
}

pub fn show_game(siv: &mut Cursive, board: Rc<RefCell<BoardState>>) {
  let board_view = draw_board(&board.borrow_mut());
  siv.add_layer(board_view);
  let board_right = board.clone();
  let board_left = board;
  siv.add_global_callback(Event::Key(Key::Right), move |siv| {
    siv.pop_layer();
    let mut board = board_right.borrow_mut();
    if board.curr_move_idx < board.moves.len() {
      board.curr_move_idx += 1;
    }
    siv.add_layer(draw_board(&board));
  });
  
  siv.add_global_callback(Event::Key(Key::Left), move |siv| {
    siv.pop_layer();
    let mut board = board_left.borrow_mut();
    if board.curr_move_idx > 0 {
      board.curr_move_idx -= 1;
    }
    siv.add_layer(draw_board(&board));
  });
}

pub fn draw_related_games_column(board_state: &BoardState) -> impl View {
  let related_board = board_state.suggestions.get(board_state.curr_move_idx).unwrap();
  let mut movement_column = LinearLayout::vertical();
  let mut games_column = LinearLayout::vertical();
  for (played_move, game) in related_board.iter().take(10) {
    let mvmt = TextView::new(if played_move.game_round % 2 == 1 {
      format!("{} . {}", played_move.game_round, played_move.san_plus.0)
    } else {
      format!("{} ... {}", played_move.game_round, played_move.san_plus.0)
    });
    let game = TextView::new(format!("{} ({}) vs {} ({})", game.white, game.white_elo.unwrap(), game.black, game.black_elo.unwrap()));
    movement_column.add_child(mvmt);
    games_column.add_child(game);
  }
  let both = LinearLayout::horizontal().child(movement_column).child(DummyView).child(games_column);
  LinearLayout::vertical().child(TextView::new(format!("{} games found", related_board.len()))).child(DummyView).child(both)
}

pub fn draw_movement_column(board_state: &BoardState) -> impl View {
  let mut white_column = LinearLayout::vertical();
  let mut black_column = LinearLayout::vertical();
  let seen     = ColorStyle::highlight();
  let not_seen = ColorStyle::secondary();
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
  ScrollView::new(LinearLayout::horizontal().child(white_column).child(black_column)).show_scrollbars(true).max_height(20)
}

pub fn draw_chess_board(board_state: &BoardState) -> impl View {
  let mut board_column = LinearLayout::vertical();
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
  board_column
}

pub fn draw_board(board_state: &BoardState) -> impl View {
  let game_description = LinearLayout::vertical()
    .child(TextView::new(format!("{} [W] vs {} [B]", board_state.game.white, board_state.game.black)))
    .child(TextView::new(format!("{} {}", board_state.game.event, board_state.game.datetime)));
  let board = draw_chess_board(board_state);
  let movement_column = draw_movement_column(board_state);
  let middle = LinearLayout::horizontal().child(Panel::new(board)).child(Panel::new(movement_column));
  let related_games = draw_related_games_column(board_state);
    
  LinearLayout::vertical().child(Panel::new(game_description)).child(Panel::new(middle)).child(Panel::new(related_games))
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

